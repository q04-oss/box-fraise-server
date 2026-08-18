use chrono::{Duration, Utc};
use serde_json::json;
use uuid::Uuid;

use crate::{
    audit,
    crypto::{b64_decode, new_nonce, verify_p256_signature},
    db::{Pool, RlsTransaction},
    domain::pairings::{repository, types::*},
    error::{AppError, AppResult},
};

/// How long a QR stays live. Short on purpose: a code that lasts an
/// hour can be photographed and redeemed from somewhere else
/// entirely, which would defeat the "you were both there" claim.
/// Matches CHALLENGE_TTL_SECS in spirit.
const NONCE_TTL_SECS: i64 = 120;

/// Ceiling on undecided pairings per person. Both-must-confirm bounds
/// the harm of someone scanning a whole room, but this keeps the list
/// and the table sane.
const MAX_PENDING_PER_USER: i64 = 25;

const MAX_DISPLAY_NAME_CHARS: usize = 40;

// ── Meeting ─────────────────────────────────────────────────────────

/// Issue a nonce for the initiator to show as a QR.
pub async fn issue_nonce(pool: &Pool, me: Uuid) -> AppResult<PairingNonceResponse> {
    let nonce = new_nonce();
    let expires_at = Utc::now() + Duration::seconds(NONCE_TTL_SECS);

    let mut tx = RlsTransaction::begin(pool, me).await?;
    repository::insert_nonce(tx.conn(), &nonce, me, None, expires_at).await?;
    tx.commit().await?;

    audit::write(
        pool,
        "user",
        Some(me),
        "pairing.nonce_issued",
        None,
        json!({}),
    )
    .await;

    Ok(PairingNonceResponse { nonce, expires_at })
}

/// Claim someone's nonce by signing it, from the iOS app. The
/// signature proves the scanner's registered device read the
/// initiator's screen inside the nonce's lifetime.
///
/// `cooling` and `window` are passed in rather than read from a
/// global, matching `onboarding::issue_challenge`. That keeps the
/// timings configurable at the edge and lets tests use one-second
/// windows without mutating process-wide state, which would race
/// under parallel tests.
pub async fn claim(
    pool: &Pool,
    me: Uuid,
    cooling: Duration,
    window: Duration,
    req: ClaimRequest,
) -> AppResult<ClaimResponse> {
    let signature_der = b64_decode(&req.signature_b64)?;
    finish_claim(pool, me, cooling, window, &req.nonce, Some(&signature_der)).await
}

/// Claim someone's nonce as a member, with no signature.
///
/// A member has no device key: their credential was handed to them
/// on a phone at the run club, by an admin who was looking at them.
/// That token is the in-person artefact, so requiring a second one
/// would only mean nobody can pair.
///
/// Everything else the signed path relies on still holds. The nonce
/// lives two minutes, so both people were in the same place at the
/// same time. The initiator asked for it while authenticated and the
/// claimer answers while authenticated, so both acted. It burns on
/// use, so one code makes one pairing.
///
/// Deliberately a separate entry point rather than "skip the check
/// when they have no keys" — a rule like that quietly becomes a hole
/// the day key registration stops being mandatory for app users.
pub async fn claim_in_person(
    pool: &Pool,
    me: Uuid,
    cooling: Duration,
    window: Duration,
    req: ClaimInPersonRequest,
) -> AppResult<ClaimResponse> {
    finish_claim(pool, me, cooling, window, &req.nonce, None).await
}

/// The part both paths share. `signature` is None for a member
/// claiming with their token.
async fn finish_claim(
    pool: &Pool,
    me: Uuid,
    cooling: Duration,
    window: Duration,
    nonce: &str,
    signature: Option<&[u8]>,
) -> AppResult<ClaimResponse> {
    let mut tx = RlsTransaction::begin(pool, me).await?;

    let (initiator_id, event_id, expires_at, used_at) = repository::get_nonce(tx.conn(), nonce)
        .await?
        .ok_or(AppError::NotFound)?;

    // Replay and expiry both mean "code no longer valid" but are
    // distinct failures, so the UI can say something useful.
    if used_at.is_some() {
        tx.rollback().await.ok();
        return Err(AppError::Conflict);
    }
    if Utc::now() >= expires_at {
        tx.rollback().await.ok();
        return Err(AppError::bad_request(
            "that code has expired — ask for a new one",
        ));
    }
    if initiator_id == me {
        tx.rollback().await.ok();
        return Err(AppError::bad_request("that is your own code"));
    }

    // The signature, when there is one, must come from one of the
    // scanner's registered device keys, over the nonce, using the same
    // P-256 / DER / SHA-256 / low-S-normalised path as event
    // verification.
    if let Some(signature_der) = signature {
        let keys = repository::public_keys_for(tx.conn(), me).await?;
        let verified = keys
            .iter()
            .any(|pk| verify_p256_signature(pk, nonce, signature_der).is_ok());
        if !verified {
            tx.rollback().await.ok();
            return Err(AppError::InvalidSignature);
        }
    }

    let pending = repository::pending_count_for(tx.conn(), me).await?;
    if pending >= MAX_PENDING_PER_USER {
        tx.rollback().await.ok();
        return Err(AppError::TooManyRequests(
            "you have a lot of connections waiting — decide on those first".into(),
        ));
    }

    if !repository::burn_nonce(tx.conn(), nonce).await? {
        // Lost the race to another claimer.
        tx.rollback().await.ok();
        return Err(AppError::Conflict);
    }

    let opens_at = Utc::now() + cooling;
    let expires = opens_at + window;
    let pairing_id =
        repository::insert_pairing(tx.conn(), me, initiator_id, event_id, opens_at, expires)
            .await?;
    tx.commit().await?;

    // Already paired. Not an error worth dressing up — they know each
    // other already.
    let pairing_id = pairing_id.ok_or(AppError::Conflict)?;

    audit::write(
        pool,
        "user",
        Some(me),
        "pairing.created",
        Some(&pairing_id.to_string()),
        json!({ "opens_at": opens_at }),
    )
    .await;

    Ok(ClaimResponse {
        pairing_id,
        opens_at,
    })
}

// ── Deciding ────────────────────────────────────────────────────────

pub async fn list(pool: &Pool, me: Uuid) -> AppResult<Vec<PairingView>> {
    let mut tx = RlsTransaction::begin(pool, me).await?;
    let rows = repository::list_for_user(tx.conn(), me).await?;
    tx.commit().await?;
    let now = Utc::now();
    Ok(rows.iter().map(|r| r.to_view(me, now)).collect())
}

/// Record the caller's answer.
///
/// Rejected before `opens_at` — there is no way to answer early, and
/// that is what makes the cooling-off real rather than cosmetic. If
/// someone could confirm at the event, the pressure the delay exists
/// to remove would simply move to that moment.
pub async fn decide(
    pool: &Pool,
    me: Uuid,
    id: Uuid,
    req: DecisionRequest,
) -> AppResult<PairingView> {
    let decision = match req.decision.trim() {
        "yes" => "yes",
        "no" => "no",
        _ => return Err(AppError::bad_request("decision must be 'yes' or 'no'")),
    };

    let mut tx = RlsTransaction::begin(pool, me).await?;
    let row = repository::get_for_user(tx.conn(), me, id)
        .await?
        .ok_or(AppError::NotFound)?;

    let now = Utc::now();
    match row.status(now) {
        "waiting" => {
            tx.rollback().await.ok();
            return Err(AppError::bad_request(
                "not yet — you can decide once the waiting period is over",
            ));
        }
        "lapsed" => {
            tx.rollback().await.ok();
            return Err(AppError::bad_request("this one has expired"));
        }
        "closed" => {
            tx.rollback().await.ok();
            return Err(AppError::Conflict);
        }
        _ => {}
    }

    repository::set_decision(tx.conn(), id, row.is_lower(me), decision).await?;
    // Only opens when both said yes. When they haven't, this is a
    // no-op and the caller learns nothing about why.
    repository::open_if_mutual(tx.conn(), id).await?;

    let updated = repository::get_for_user(tx.conn(), me, id)
        .await?
        .ok_or(AppError::NotFound)?;
    tx.commit().await?;

    // Deliberately does NOT record which way. The audit table is
    // admin-readable and there is no operational need for the value;
    // recording it would be one more place the silent-decline
    // property could leak.
    audit::write(
        pool,
        "user",
        Some(me),
        "pairing.decided",
        Some(&id.to_string()),
        json!({}),
    )
    .await;

    Ok(updated.to_view(me, Utc::now()))
}

/// Close a pairing permanently. Unlike declining, this is observable
/// — the other person's messages stop being accepted. That asymmetry
/// is deliberate: a decline happens before any relationship exists
/// and should cost nothing; a block ends one that does, and someone
/// shouting into a void is entitled to know the conversation is over.
pub async fn block(pool: &Pool, me: Uuid, id: Uuid) -> AppResult<()> {
    let mut tx = RlsTransaction::begin(pool, me).await?;
    let row = repository::get_for_user(tx.conn(), me, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let closed = repository::close(tx.conn(), row.id, me).await?;
    tx.commit().await?;

    if !closed {
        return Err(AppError::Conflict);
    }

    audit::write(
        pool,
        "user",
        Some(me),
        "pairing.blocked",
        Some(&id.to_string()),
        json!({}),
    )
    .await;
    Ok(())
}

/// The gate the chat service asks about.
pub async fn authorized(pool: &Pool, me: Uuid, peer: Uuid) -> AppResult<AuthorizedResponse> {
    if me == peer {
        return Ok(AuthorizedResponse { authorized: false });
    }
    let mut tx = RlsTransaction::begin(pool, me).await?;
    let ok = repository::is_authorized(tx.conn(), me, peer).await?;
    tx.commit().await?;
    Ok(AuthorizedResponse { authorized: ok })
}

// ── Display name ────────────────────────────────────────────────────

/// The label the other person sees. Not verified, not unique — the
/// identifier is still the key.
pub async fn set_display_name(pool: &Pool, me: Uuid, req: SetDisplayNameRequest) -> AppResult<()> {
    let name = req.display_name.trim();
    if name.is_empty() {
        return Err(AppError::bad_request("name required"));
    }
    if name.chars().count() > MAX_DISPLAY_NAME_CHARS {
        return Err(AppError::bad_request(format!(
            "name must be {MAX_DISPLAY_NAME_CHARS} characters or fewer"
        )));
    }

    let mut tx = RlsTransaction::begin(pool, me).await?;
    repository::set_display_name(tx.conn(), me, name).await?;
    tx.commit().await?;
    Ok(())
}
