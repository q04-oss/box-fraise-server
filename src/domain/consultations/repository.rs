use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use super::types::{IdentityCard, SocialVerification};

pub async fn insert_verification(
    conn: &mut PgConnection,
    user_id: Uuid,
    consulted_by_user_id: Uuid,
    salon_id: Option<Uuid>,
    consultation_notes: Option<&str>,
    consent_snapshot: &Value,
) -> sqlx::Result<SocialVerification> {
    let row = sqlx::query_as::<_, SocialVerification>(
        "INSERT INTO social_verifications
            (user_id, consulted_by_user_id, salon_id,
             consultation_notes, consent_snapshot)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING id, user_id, consulted_by_user_id, salon_id,
                   consulted_at, consultation_notes, consent_snapshot,
                   status, created_at",
    )
    .bind(user_id)
    .bind(consulted_by_user_id)
    .bind(salon_id)
    .bind(consultation_notes)
    .bind(consent_snapshot)
    .fetch_one(conn)
    .await?;
    Ok(row)
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_card(
    conn: &mut PgConnection,
    user_id: Uuid,
    social_verification_id: Uuid,
    serial: &str,
    issued_by_user_id: Uuid,
    salon_id: Option<Uuid>,
    design_version: &str,
) -> sqlx::Result<IdentityCard> {
    let row = sqlx::query_as::<_, IdentityCard>(
        "INSERT INTO identity_cards
            (user_id, social_verification_id, serial,
             issued_by_user_id, salon_id, design_version)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING id, user_id, social_verification_id, serial,
                   issued_at, issued_by_user_id, salon_id, design_version,
                   status, replaced_by_card_id, revoked_at, revoked_reason,
                   created_at",
    )
    .bind(user_id)
    .bind(social_verification_id)
    .bind(serial)
    .bind(issued_by_user_id)
    .bind(salon_id)
    .bind(design_version)
    .fetch_one(conn)
    .await?;
    Ok(row)
}

pub async fn get_card_by_serial(
    conn: &mut PgConnection,
    serial: &str,
) -> sqlx::Result<Option<IdentityCard>> {
    let row = sqlx::query_as::<_, IdentityCard>(
        "SELECT id, user_id, social_verification_id, serial,
                issued_at, issued_by_user_id, salon_id, design_version,
                status, replaced_by_card_id, revoked_at, revoked_reason,
                created_at
           FROM identity_cards
          WHERE serial = $1",
    )
    .bind(serial)
    .fetch_optional(conn)
    .await?;
    Ok(row)
}

pub async fn get_card_by_id(
    conn: &mut PgConnection,
    id: Uuid,
) -> sqlx::Result<Option<IdentityCard>> {
    let row = sqlx::query_as::<_, IdentityCard>(
        "SELECT id, user_id, social_verification_id, serial,
                issued_at, issued_by_user_id, salon_id, design_version,
                status, replaced_by_card_id, revoked_at, revoked_reason,
                created_at
           FROM identity_cards
          WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(conn)
    .await?;
    Ok(row)
}

pub async fn revoke_card(
    conn: &mut PgConnection,
    id: Uuid,
    reason: &str,
    at: DateTime<Utc>,
) -> sqlx::Result<Option<IdentityCard>> {
    let row = sqlx::query_as::<_, IdentityCard>(
        "UPDATE identity_cards
            SET status         = 'revoked',
                revoked_at     = $1,
                revoked_reason = $2
          WHERE id = $3 AND status = 'active'
          RETURNING id, user_id, social_verification_id, serial,
                    issued_at, issued_by_user_id, salon_id, design_version,
                    status, replaced_by_card_id, revoked_at, revoked_reason,
                    created_at",
    )
    .bind(at)
    .bind(reason)
    .bind(id)
    .fetch_optional(conn)
    .await?;
    Ok(row)
}

pub async fn mark_card_replaced(
    conn: &mut PgConnection,
    old_card_id: Uuid,
    new_card_id: Uuid,
) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE identity_cards
            SET status              = 'replaced',
                replaced_by_card_id = $1
          WHERE id = $2",
    )
    .bind(new_card_id)
    .bind(old_card_id)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn latest_verification_for(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<Option<SocialVerification>> {
    let row = sqlx::query_as::<_, SocialVerification>(
        "SELECT id, user_id, consulted_by_user_id, salon_id,
                consulted_at, consultation_notes, consent_snapshot,
                status, created_at
           FROM social_verifications
          WHERE user_id = $1 AND status = 'verified'
          ORDER BY consulted_at DESC
          LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(conn)
    .await?;
    Ok(row)
}

pub async fn active_card_for(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<Option<IdentityCard>> {
    let row = sqlx::query_as::<_, IdentityCard>(
        "SELECT id, user_id, social_verification_id, serial,
                issued_at, issued_by_user_id, salon_id, design_version,
                status, replaced_by_card_id, revoked_at, revoked_reason,
                created_at
           FROM identity_cards
          WHERE user_id = $1 AND status = 'active'
          ORDER BY issued_at DESC
          LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(conn)
    .await?;
    Ok(row)
}
