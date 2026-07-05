use chrono::Utc;
use rand::RngCore;
use serde_json::json;
use uuid::Uuid;

use crate::{
    audit,
    db::{AdminRlsTransaction, Pool, RlsTransaction},
    domain::consultations::{repository, types::*},
    error::{AppError, AppResult},
};

// ── Serial generation ──────────────────────────────────────────────
//
// Fully random, no salon prefix, no sequence. 10 random bytes → 20
// hex chars → grouped as 5×4 with hyphens: A3F4-9B2C-8D01-5E76-4A9F.
// 80 bits of entropy — enumeration is impossible.

fn generate_serial() -> String {
    let mut bytes = [0u8; 10];
    rand::thread_rng().fill_bytes(&mut bytes);
    let hex: String = bytes.iter().map(|b| format!("{b:02X}")).collect();
    let chars: Vec<char> = hex.chars().collect();
    let mut out = String::with_capacity(chars.len() + 4);
    for (i, c) in chars.iter().enumerate() {
        if i > 0 && i % 4 == 0 {
            out.push('-');
        }
        out.push(*c);
    }
    out
}

/// Normalize a serial for lookup: strip hyphens + whitespace,
/// uppercase, then re-format canonically. Accepts variants like
/// "a3f4 9b2c 8d015e764a9f" from copy-paste and canonicalises them.
pub fn canonicalise_serial(input: &str) -> Option<String> {
    let stripped: String = input
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_uppercase())
        .collect();
    if stripped.len() != 20 || !stripped.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let chars: Vec<char> = stripped.chars().collect();
    let mut out = String::with_capacity(24);
    for (i, c) in chars.iter().enumerate() {
        if i > 0 && i % 4 == 0 {
            out.push('-');
        }
        out.push(*c);
    }
    Some(out)
}

// ── Complete consultation + issue card (atomic) ─────────────────────

pub async fn complete_consultation(
    pool: &Pool,
    consultant_user_id: Uuid,
    req: CompleteConsultationRequest,
) -> AppResult<CompleteConsultationResponse> {
    if req.user_id == consultant_user_id {
        return Err(AppError::bad_request(
            "a consultant cannot verify themselves",
        ));
    }

    // Enforce operational safety: whoever is completing this must be a
    // trained consultant. We check the staff table under admin context.
    ensure_can_consult(pool, consultant_user_id).await?;

    let serial = generate_serial();

    // Both writes go in the same admin transaction so the two records
    // land together or not at all.
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let verification = repository::insert_verification(
        tx.conn(),
        req.user_id,
        consultant_user_id,
        req.salon_id,
        req.consultation_notes.as_deref(),
        &req.consent_snapshot,
    )
    .await?;
    let card = repository::insert_card(
        tx.conn(),
        req.user_id,
        verification.id,
        &serial,
        consultant_user_id,
        req.salon_id,
        &req.design_version,
    )
    .await?;
    tx.commit().await?;

    // Audit outside the tx, as always.
    audit::write(
        pool,
        "admin",
        Some(consultant_user_id),
        "consultation.complete",
        Some(&req.user_id.to_string()),
        json!({
            "verification_id": verification.id,
            "card_id":         card.id,
            "card_serial":     card.serial,
            "salon_id":        req.salon_id,
        }),
    )
    .await;

    Ok(CompleteConsultationResponse { verification, card })
}

type StaffTrainingRow = (
    Option<chrono::DateTime<chrono::Utc>>, // consultation_training_completed_at
    Option<chrono::DateTime<chrono::Utc>>, // terminated_at
);

async fn ensure_can_consult(pool: &Pool, user_id: Uuid) -> AppResult<()> {
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let row: Option<StaffTrainingRow> = sqlx::query_as(
        "SELECT consultation_training_completed_at, terminated_at
           FROM staff
          WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(tx.conn())
    .await?;
    tx.commit().await?;

    match row {
        None => Err(AppError::Forbidden),
        Some((_, Some(_terminated))) => Err(AppError::Forbidden),
        Some((None, _)) => Err(AppError::Forbidden),
        Some((Some(_trained), None)) => Ok(()),
    }
}

// ── Card revoke / replace ───────────────────────────────────────────

pub async fn revoke_card(
    pool: &Pool,
    actor_user_id: Uuid,
    card_id: Uuid,
    req: RevokeCardRequest,
) -> AppResult<IdentityCard> {
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let updated = repository::revoke_card(tx.conn(), card_id, &req.reason, Utc::now()).await?;
    tx.commit().await?;
    let card = updated.ok_or(AppError::NotFound)?;

    audit::write(
        pool,
        "admin",
        Some(actor_user_id),
        "card.revoke",
        Some(&card.id.to_string()),
        json!({ "reason": req.reason, "serial": card.serial }),
    )
    .await;
    Ok(card)
}

pub async fn replace_card(
    pool: &Pool,
    actor_user_id: Uuid,
    old_card_id: Uuid,
    req: ReplaceCardRequest,
) -> AppResult<IdentityCard> {
    // Load the old card to find its owning user and verification.
    let old = {
        let mut tx = AdminRlsTransaction::begin(pool).await?;
        let card = repository::get_card_by_id(tx.conn(), old_card_id).await?;
        tx.commit().await?;
        card.ok_or(AppError::NotFound)?
    };
    if old.status != "active" {
        return Err(AppError::Conflict);
    }

    let design_version = req
        .design_version
        .unwrap_or_else(|| old.design_version.clone());
    let serial = generate_serial();

    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let new_card = repository::insert_card(
        tx.conn(),
        old.user_id,
        old.social_verification_id,
        &serial,
        actor_user_id,
        old.salon_id,
        &design_version,
    )
    .await?;
    repository::mark_card_replaced(tx.conn(), old.id, new_card.id).await?;
    tx.commit().await?;

    audit::write(
        pool,
        "admin",
        Some(actor_user_id),
        "card.replace",
        Some(&new_card.id.to_string()),
        json!({
            "old_card_id":  old.id,
            "old_serial":   old.serial,
            "new_serial":   new_card.serial,
        }),
    )
    .await;
    Ok(new_card)
}

// ── Public card lookup ──────────────────────────────────────────────

pub async fn lookup_card(pool: &Pool, serial_input: &str) -> AppResult<CardLookupResponse> {
    let serial =
        canonicalise_serial(serial_input).ok_or_else(|| AppError::bad_request("invalid serial"))?;

    // No user context — this is called from a scanner or open browser.
    // The identity_cards SELECT policy permits USING(true); the audit
    // boundary is right here: we return only non-identifying fields.
    let card = sqlx::query_as::<_, IdentityCard>(
        "SELECT id, user_id, social_verification_id, serial,
                issued_at, issued_by_user_id, salon_id, design_version,
                status, replaced_by_card_id, revoked_at, revoked_reason,
                created_at
           FROM identity_cards
          WHERE serial = $1",
    )
    .bind(&serial)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)?;

    Ok(CardLookupResponse {
        serial: card.serial,
        is_valid: card.status == "active",
        status: card.status,
        issued_at: card.issued_at,
        design_version: card.design_version,
    })
}

// ── User: check my verification status ──────────────────────────────

pub async fn my_verification_status(pool: &Pool, user_id: Uuid) -> AppResult<MyVerificationStatus> {
    let mut tx = RlsTransaction::begin(pool, user_id).await?;
    let verification = repository::latest_verification_for(tx.conn(), user_id).await?;
    let card = repository::active_card_for(tx.conn(), user_id).await?;
    tx.commit().await?;

    let tier: u8 = if verification.is_some() { 2 } else { 1 };
    let verified = verification.is_some();

    Ok(MyVerificationStatus {
        tier,
        verified,
        verification,
        active_card: card,
    })
}
