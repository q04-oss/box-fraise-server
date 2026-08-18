use sqlx::PgConnection;
use uuid::Uuid;

/// Create a verified member.
///
/// There is no pending step. The iOS onboarding path registers first
/// and is verified later, because the device proves itself before an
/// admin ever sees it. This path is the other way round: an admin is
/// looking at the person as they type, so the verification has already
/// happened by the time the row exists.
///
/// The three verification columns are set together because the users
/// CHECK constraint requires it — a verified row without an event and
/// an admin is not representable, which is what keeps "verified" from
/// quietly meaning less than it says.
pub async fn insert_verified_member(
    conn: &mut PgConnection,
    event_id: Uuid,
    admin_id: Uuid,
) -> sqlx::Result<(Uuid, i32)> {
    sqlx::query_as(
        "INSERT INTO users
             (status, verified_at, verified_at_event_id, verified_by_admin_id, member_no)
         VALUES ('verified', now(), $1, $2, nextval('member_no_seq'))
         RETURNING id, member_no",
    )
    .bind(event_id)
    .bind(admin_id)
    .fetch_one(conn)
    .await
}

pub async fn insert_session(
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

/// The number that goes on a post. Read under the poster's own
/// context, which `users_self_or_admin_select` permits.
pub async fn member_no(conn: &mut PgConnection, user_id: Uuid) -> sqlx::Result<Option<i32>> {
    sqlx::query_scalar::<_, Option<i32>>("SELECT member_no FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(conn)
        .await
        .map(Option::flatten)
}
