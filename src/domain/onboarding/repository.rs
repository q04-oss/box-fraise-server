// SQL-only — the service layer owns transactions and policy decisions.

use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

/// Insert a fresh pending user with a caller-supplied id. We pre-generate
/// the UUID in Rust so the calling transaction can set app.user_id BEFORE
/// the INSERT runs — that satisfies the users SELECT policy on
/// `INSERT ... RETURNING` and keeps the rest of the transaction scoped
/// under the same identity. Without this, INSERT RETURNING fails:
/// the SELECT policy refuses to "see" the new row to return it.
pub async fn insert_pending_user_with_id(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<()> {
    sqlx::query("INSERT INTO users (id) VALUES ($1)")
        .bind(user_id)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn insert_device_key(
    conn: &mut PgConnection,
    user_id: Uuid,
    public_key: &[u8],
    key_id: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO device_keys (user_id, public_key, key_id)
         VALUES ($1, $2, $3)",
    )
    .bind(user_id)
    .bind(public_key)
    .bind(key_id)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn insert_user_session(
    conn: &mut PgConnection,
    user_id: Uuid,
    token_hash: &str,
) -> sqlx::Result<()> {
    sqlx::query("INSERT INTO user_sessions (token_hash, user_id) VALUES ($1, $2)")
        .bind(token_hash)
        .bind(user_id)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn insert_challenge(
    conn: &mut PgConnection,
    user_id: Uuid,
    nonce: &str,
    expires_at: DateTime<Utc>,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO challenges (nonce, user_id, expires_at)
         VALUES ($1, $2, $3)",
    )
    .bind(nonce)
    .bind(user_id)
    .bind(expires_at)
    .execute(conn)
    .await?;
    Ok(())
}

/// Returns (user_id, expires_at, used_at) for a challenge, if present.
pub async fn get_challenge_for_verify(
    conn: &mut PgConnection,
    nonce: &str,
) -> sqlx::Result<Option<(Uuid, DateTime<Utc>, Option<DateTime<Utc>>)>> {
    let row: Option<(Uuid, DateTime<Utc>, Option<DateTime<Utc>>)> =
        sqlx::query_as("SELECT user_id, expires_at, used_at FROM challenges WHERE nonce = $1")
            .bind(nonce)
            .fetch_optional(conn)
            .await?;
    Ok(row)
}

pub async fn mark_challenge_used(conn: &mut PgConnection, nonce: &str) -> sqlx::Result<()> {
    sqlx::query("UPDATE challenges SET used_at = now() WHERE nonce = $1")
        .bind(nonce)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn get_device_public_key(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<Option<Vec<u8>>> {
    let row: Option<(Vec<u8>,)> =
        sqlx::query_as("SELECT public_key FROM device_keys WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(conn)
            .await?;
    Ok(row.map(|(pk,)| pk))
}

/// Atomic verify. Returns Some(verified_at) iff the row was `pending`
/// and was successfully flipped to `verified`; None if the row was
/// already verified or missing. The caller maps None → 409 Conflict.
///
/// This is the single statement that closes the race window: two admins
/// scanning the same user simultaneously will see one UPDATE return a
/// row and the other return zero.
pub async fn promote_user_to_verified(
    conn: &mut PgConnection,
    user_id: Uuid,
    event_id: Uuid,
    admin_id: Uuid,
) -> sqlx::Result<Option<DateTime<Utc>>> {
    let row: Option<(DateTime<Utc>,)> = sqlx::query_as(
        "UPDATE users
            SET status='verified',
                verified_at=now(),
                verified_at_event_id=$1,
                verified_by_admin_id=$2
          WHERE id=$3 AND status='pending'
          RETURNING verified_at",
    )
    .bind(event_id)
    .bind(admin_id)
    .bind(user_id)
    .fetch_optional(conn)
    .await?;
    Ok(row.map(|(t,)| t))
}

pub async fn get_user_me(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<Option<(String, Option<DateTime<Utc>>, Option<Uuid>)>> {
    let row: Option<(String, Option<DateTime<Utc>>, Option<Uuid>)> = sqlx::query_as(
        "SELECT status, verified_at, verified_at_event_id
           FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(conn)
    .await?;
    Ok(row)
}
