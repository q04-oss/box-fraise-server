// Onboarding service.
//
// Three transactions, three contexts:
//   - register:        RlsTransaction::begin_anonymous (no identity yet)
//   - issue_challenge: RlsTransaction::begin(user_id)
//   - verify:          AdminRlsTransaction::begin
//
// Every audit::write call lives OUTSIDE the request transaction so the
// trail survives a rollback.

use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use crate::{
    audit,
    crypto::{b64_decode, new_nonce, new_session_token, verify_p256_signature},
    db::{AdminRlsTransaction, Pool, RlsTransaction},
    domain::onboarding::{repository, types::*},
    error::{AppError, AppResult},
};

// The user_id is pre-generated in Rust (not via DEFAULT gen_random_uuid())
// for one specific reason: we need to set app.user_id BEFORE the
// INSERT so the SELECT policy permits the RETURNING and so the
// subsequent device_key / user_session INSERTs run under user-scoped
// context. See repository::insert_pending_user_with_id for the longer
// note.

const SEC1_UNCOMPRESSED_LEN: usize = 65; // 0x04 || X(32) || Y(32)

pub async fn register(pool: &Pool, req: RegisterRequest) -> AppResult<RegisterResponse> {
    // Validate at the boundary — bad input never reaches a SQL bind.
    let pk_bytes = b64_decode(&req.public_key)?;
    if pk_bytes.len() != SEC1_UNCOMPRESSED_LEN || pk_bytes[0] != 0x04 {
        return Err(AppError::bad_request(
            "public_key must be SEC1 uncompressed (65 bytes, 0x04 prefix)",
        ));
    }
    if p256::PublicKey::from_sec1_bytes(&pk_bytes).is_err() {
        return Err(AppError::bad_request(
            "public_key is not a valid P-256 point",
        ));
    }
    let key_id = req.key_id.trim();
    if key_id.is_empty() || key_id.len() > 256 {
        return Err(AppError::bad_request("key_id must be 1..=256 chars"));
    }

    let user_id = Uuid::new_v4();
    let (token, token_hash) = new_session_token();

    let mut tx = RlsTransaction::begin(pool, user_id).await?;
    repository::insert_pending_user_with_id(tx.conn(), user_id).await?;
    repository::insert_device_key(tx.conn(), user_id, &pk_bytes, key_id).await?;
    repository::insert_user_session(tx.conn(), user_id, &token_hash).await?;
    tx.commit().await?;

    audit::write(
        pool,
        "system",
        None,
        "user.register",
        Some(&user_id.to_string()),
        json!({ "key_id": key_id }),
    )
    .await;

    Ok(RegisterResponse {
        user_id,
        status: "pending".into(),
        session_token: token,
    })
}

pub async fn issue_challenge(
    pool: &Pool,
    ttl: chrono::Duration,
    user_id: Uuid,
) -> AppResult<ChallengeResponse> {
    let nonce = new_nonce();
    let expires_at = Utc::now() + ttl;

    let mut tx = RlsTransaction::begin(pool, user_id).await?;
    repository::insert_challenge(tx.conn(), user_id, &nonce, expires_at).await?;
    tx.commit().await?;

    audit::write(
        pool,
        "user",
        Some(user_id),
        "challenge.issued",
        Some(&nonce),
        json!({}),
    )
    .await;

    Ok(ChallengeResponse { nonce, expires_at })
}

pub async fn verify(pool: &Pool, admin_id: Uuid, req: VerifyRequest) -> AppResult<VerifyResponse> {
    let signature_der = b64_decode(&req.signature_b64)?;

    let mut tx = AdminRlsTransaction::begin(pool).await?;

    let (user_id, expires_at, used_at) =
        repository::get_challenge_for_verify(tx.conn(), &req.nonce)
            .await?
            .ok_or(AppError::NotFound)?;

    // Order matters: replay (used) and expiry both look like
    // "challenge no longer valid" but produce distinct error codes so
    // the client UX can tell them apart.
    if used_at.is_some() {
        return Err(AppError::Conflict);
    }
    if expires_at <= Utc::now() {
        return Err(AppError::bad_request("challenge expired"));
    }

    // A user can have multiple device keys bound (post-0002 migration):
    // browser, phone, hardware card. Any one of them signing the nonce
    // is proof of presence. Iterate and accept the first match.
    let public_keys = repository::get_device_public_keys(tx.conn(), user_id).await?;
    if public_keys.is_empty() {
        return Err(AppError::NotFound);
    }
    let any_match = public_keys
        .iter()
        .any(|pk| verify_p256_signature(pk, &req.nonce, &signature_der).is_ok());
    if !any_match {
        // Apple Secure Enclave may emit high-S; verify_p256_signature
        // normalises before checking. See crypto::verify_p256_signature.
        return Err(AppError::InvalidSignature);
    }

    let verified_at =
        repository::promote_user_to_verified(tx.conn(), user_id, req.event_id, admin_id)
            .await?
            .ok_or(AppError::Conflict)?;

    repository::mark_challenge_used(tx.conn(), &req.nonce).await?;
    tx.commit().await?;

    audit::write(
        pool,
        "admin",
        Some(admin_id),
        "user.verify",
        Some(&user_id.to_string()),
        json!({
            "event_id": req.event_id,
            "nonce": req.nonce,
        }),
    )
    .await;

    Ok(VerifyResponse {
        user_id,
        status: "verified".into(),
        verified_at,
        verified_at_event_id: req.event_id,
    })
}

pub async fn me(pool: &Pool, user_id: Uuid) -> AppResult<MeResponse> {
    let mut tx = RlsTransaction::begin(pool, user_id).await?;
    let row = repository::get_user_me(tx.conn(), user_id)
        .await?
        .ok_or(AppError::NotFound)?;
    tx.commit().await?;
    Ok(MeResponse {
        id: user_id,
        status: row.status,
        verified_at: row.verified_at,
        event: row.event,
    })
}
