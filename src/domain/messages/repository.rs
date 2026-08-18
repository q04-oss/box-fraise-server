use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

/// Publish or replace a member's public key. Replacing abandons every
/// conversation they had — the shared secrets came from the old one —
/// which is why this is an explicit act rather than a side effect of
/// opening the page on a new device.
pub async fn upsert_key(
    conn: &mut PgConnection,
    user_id: Uuid,
    public_key: &[u8],
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO member_keys (user_id, public_key)
         VALUES ($1, $2)
         ON CONFLICT (user_id) DO UPDATE
            SET public_key = EXCLUDED.public_key, created_at = now()",
    )
    .bind(user_id)
    .bind(public_key)
    .execute(conn)
    .await?;
    Ok(())
}

/// The other person's key. RLS returns nothing unless the caller has
/// an open channel with them, so this needs no ownership check of its
/// own — see 0026.
pub async fn peer_key(conn: &mut PgConnection, peer_id: Uuid) -> sqlx::Result<Option<Vec<u8>>> {
    sqlx::query_scalar::<_, Vec<u8>>("SELECT public_key FROM member_keys WHERE user_id = $1")
        .bind(peer_id)
        .fetch_optional(conn)
        .await
}

pub async fn insert_message(
    conn: &mut PgConnection,
    pairing_id: Uuid,
    sender_id: Uuid,
    ciphertext: &[u8],
    iv: &[u8],
) -> sqlx::Result<(Uuid, DateTime<Utc>)> {
    sqlx::query_as(
        "INSERT INTO messages (pairing_id, sender_id, ciphertext, iv)
         VALUES ($1, $2, $3, $4)
         RETURNING id, created_at",
    )
    .bind(pairing_id)
    .bind(sender_id)
    .bind(ciphertext)
    .bind(iv)
    .fetch_one(conn)
    .await
}

/// The conversation, oldest first. RLS hides it entirely unless the
/// pairing is open to the caller.
pub async fn list_messages(
    conn: &mut PgConnection,
    pairing_id: Uuid,
) -> sqlx::Result<Vec<(Uuid, Uuid, Vec<u8>, Vec<u8>, DateTime<Utc>)>> {
    sqlx::query_as(
        "SELECT id, sender_id, ciphertext, iv, created_at
           FROM messages
          WHERE pairing_id = $1
          ORDER BY created_at ASC",
    )
    .bind(pairing_id)
    .fetch_all(conn)
    .await
}
