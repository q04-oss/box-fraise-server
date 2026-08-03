use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

use super::types::PairingRow;

/// The columns every pairing read shares.
///
/// It selects BOTH decision columns because the service needs the
/// caller's own, and which one that is depends on which side they
/// are. `PairingRow` is internal and never serialized; `to_view`
/// projects it to the type that crosses the wire, which has nowhere
/// to put the counterparty's value. Do not add a `Serialize` derive
/// to `PairingRow`.
const PAIRING_COLUMNS: &str = "
    p.id, p.lower_user_id, p.upper_user_id,
    p.lower_decision, p.upper_decision,
    p.met_at, p.opens_at, p.expires_at, p.opened_at, p.closed_at,
    bf_peer_display_name(
        $1,
        CASE WHEN p.lower_user_id = $1 THEN p.upper_user_id ELSE p.lower_user_id END
    ) AS peer_name,
    e.name AS event_name
";

/// No join to `users`. The counterparty's row is invisible under
/// `users_self_or_admin_select`, so joining it would match nothing and
/// — being an inner join — drop the pairing entirely. Under RLS a join
/// is also a filter. The name comes from the SECURITY DEFINER function
/// added in 0015, which returns one column and only between people who
/// are actually paired.
///
/// The events join stays LEFT: an event can be unpublished later, and
/// a pairing should not vanish because the party it came from did.
const PAIRING_FROM: &str = "
    FROM pairings p
    LEFT JOIN events e ON e.id = p.event_id
";

pub async fn insert_nonce(
    conn: &mut PgConnection,
    nonce: &str,
    initiator_id: Uuid,
    event_id: Option<Uuid>,
    expires_at: DateTime<Utc>,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO pairing_nonces (nonce, initiator_id, event_id, expires_at)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(nonce)
    .bind(initiator_id)
    .bind(event_id)
    .bind(expires_at)
    .execute(conn)
    .await?;
    Ok(())
}

/// Resolve a nonce. Returns (initiator_id, event_id, expires_at,
/// used_at) so the service can distinguish expiry from replay.
#[allow(clippy::type_complexity)]
pub async fn get_nonce(
    conn: &mut PgConnection,
    nonce: &str,
) -> sqlx::Result<Option<(Uuid, Option<Uuid>, DateTime<Utc>, Option<DateTime<Utc>>)>> {
    sqlx::query_as(
        "SELECT initiator_id, event_id, expires_at, used_at
           FROM pairing_nonces WHERE nonce = $1",
    )
    .bind(nonce)
    .fetch_optional(conn)
    .await
}

/// Burn the nonce. `AND used_at IS NULL` is the race-close: two
/// simultaneous claims produce exactly one winner.
pub async fn burn_nonce(conn: &mut PgConnection, nonce: &str) -> sqlx::Result<bool> {
    let burned: Option<(String,)> = sqlx::query_as(
        "UPDATE pairing_nonces SET used_at = now()
          WHERE nonce = $1 AND used_at IS NULL
          RETURNING nonce",
    )
    .bind(nonce)
    .fetch_optional(conn)
    .await?;
    Ok(burned.is_some())
}

/// Insert a pairing with the ids sorted so the unique index does its
/// job. Returns None on unique violation — meaning these two are
/// already paired.
pub async fn insert_pairing(
    conn: &mut PgConnection,
    a: Uuid,
    b: Uuid,
    event_id: Option<Uuid>,
    opens_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
) -> sqlx::Result<Option<Uuid>> {
    let (lower, upper) = if a < b { (a, b) } else { (b, a) };
    let result: Result<(Uuid,), sqlx::Error> = sqlx::query_as(
        "INSERT INTO pairings
             (lower_user_id, upper_user_id, event_id, opens_at, expires_at)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING id",
    )
    .bind(lower)
    .bind(upper)
    .bind(event_id)
    .bind(opens_at)
    .bind(expires_at)
    .fetch_one(conn)
    .await;

    match result {
        Ok((id,)) => Ok(Some(id)),
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => Ok(None),
        Err(e) => Err(e),
    }
}

pub async fn list_for_user(conn: &mut PgConnection, me: Uuid) -> sqlx::Result<Vec<PairingRow>> {
    sqlx::query_as::<_, PairingRow>(&format!(
        "SELECT {PAIRING_COLUMNS} {PAIRING_FROM}
          WHERE p.lower_user_id = $1 OR p.upper_user_id = $1
          ORDER BY p.met_at DESC"
    ))
    .bind(me)
    .fetch_all(conn)
    .await
}

pub async fn get_for_user(
    conn: &mut PgConnection,
    me: Uuid,
    id: Uuid,
) -> sqlx::Result<Option<PairingRow>> {
    sqlx::query_as::<_, PairingRow>(&format!(
        "SELECT {PAIRING_COLUMNS} {PAIRING_FROM}
          WHERE p.id = $2 AND (p.lower_user_id = $1 OR p.upper_user_id = $1)"
    ))
    .bind(me)
    .bind(id)
    .fetch_optional(conn)
    .await
}

/// Record one side's answer. `is_lower` picks the column; the caller's
/// own side only, never the other.
pub async fn set_decision(
    conn: &mut PgConnection,
    id: Uuid,
    is_lower: bool,
    decision: &str,
) -> sqlx::Result<()> {
    let sql = if is_lower {
        "UPDATE pairings SET lower_decision = $2, lower_decided_at = now() WHERE id = $1"
    } else {
        "UPDATE pairings SET upper_decision = $2, upper_decided_at = now() WHERE id = $1"
    };
    sqlx::query(sql)
        .bind(id)
        .bind(decision)
        .execute(conn)
        .await?;
    Ok(())
}

/// Open the channel iff both sides said yes and the window has been
/// reached. `AND opened_at IS NULL` is the race-close: two
/// simultaneous confirmations produce exactly one open.
pub async fn open_if_mutual(
    conn: &mut PgConnection,
    id: Uuid,
) -> sqlx::Result<Option<DateTime<Utc>>> {
    sqlx::query_scalar::<_, DateTime<Utc>>(
        "UPDATE pairings SET opened_at = now()
          WHERE id = $1
            AND opened_at IS NULL
            AND closed_at IS NULL
            AND now() >= opens_at
            AND now() < expires_at
            AND lower_decision = 'yes'
            AND upper_decision = 'yes'
          RETURNING opened_at",
    )
    .bind(id)
    .fetch_optional(conn)
    .await
}

pub async fn close(conn: &mut PgConnection, id: Uuid, by: Uuid) -> sqlx::Result<bool> {
    let closed: Option<(Uuid,)> = sqlx::query_as(
        "UPDATE pairings SET closed_at = now(), closed_by = $2
          WHERE id = $1 AND closed_at IS NULL
          RETURNING id",
    )
    .bind(id)
    .bind(by)
    .fetch_optional(conn)
    .await?;
    Ok(closed.is_some())
}

/// Is there an open, unclosed pairing between these two? This is what
/// the chat service asks before letting a message through.
pub async fn is_authorized(conn: &mut PgConnection, a: Uuid, b: Uuid) -> sqlx::Result<bool> {
    let (lower, upper) = if a < b { (a, b) } else { (b, a) };
    let found: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM pairings
          WHERE lower_user_id = $1 AND upper_user_id = $2
            AND opened_at IS NOT NULL AND closed_at IS NULL",
    )
    .bind(lower)
    .bind(upper)
    .fetch_optional(conn)
    .await?;
    Ok(found.is_some())
}

/// How many pairings this user already has that have not opened.
/// Bounds someone working a room scanning everybody.
pub async fn pending_count_for(conn: &mut PgConnection, me: Uuid) -> sqlx::Result<i64> {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)::bigint FROM pairings
          WHERE (lower_user_id = $1 OR upper_user_id = $1)
            AND opened_at IS NULL AND closed_at IS NULL
            AND now() < expires_at",
    )
    .bind(me)
    .fetch_one(conn)
    .await
}

pub async fn set_display_name(conn: &mut PgConnection, me: Uuid, name: &str) -> sqlx::Result<()> {
    sqlx::query("UPDATE users SET display_name = $2 WHERE id = $1")
        .bind(me)
        .bind(name)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn public_keys_for(conn: &mut PgConnection, user_id: Uuid) -> sqlx::Result<Vec<Vec<u8>>> {
    let rows: Vec<(Vec<u8>,)> =
        sqlx::query_as("SELECT public_key FROM device_keys WHERE user_id = $1")
            .bind(user_id)
            .fetch_all(conn)
            .await?;
    Ok(rows.into_iter().map(|(pk,)| pk).collect())
}
