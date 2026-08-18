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

/// End one session. Run under the member's own context, where 0027's
/// `user_sessions_own_delete` policy scopes the delete to their rows —
/// so a token hash belonging to somebody else matches nothing, rather
/// than logging a stranger out.
///
/// Returns false when the row was already gone, which is what a second
/// tap on sign-out looks like.
pub async fn delete_session(conn: &mut PgConnection, token_hash: &str) -> sqlx::Result<bool> {
    let done = sqlx::query("DELETE FROM user_sessions WHERE token_hash = $1")
        .bind(token_hash)
        .execute(conn)
        .await?;
    Ok(done.rows_affected() == 1)
}

/// End every session a member has. Run under an admin's context, and
/// used for one thing: somebody standing at a run saying they lost
/// their phone. Whatever is on that phone stops working before the
/// replacement is minted.
pub async fn delete_all_sessions(conn: &mut PgConnection, user_id: Uuid) -> sqlx::Result<u64> {
    let done = sqlx::query("DELETE FROM user_sessions WHERE user_id = $1")
        .bind(user_id)
        .execute(conn)
        .await?;
    Ok(done.rows_affected())
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

/// Find a member by the number they read out. None when nobody has it.
pub async fn id_by_member_no(
    conn: &mut PgConnection,
    member_no: i32,
) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar::<_, Uuid>("SELECT id FROM users WHERE member_no = $1")
        .bind(member_no)
        .fetch_optional(conn)
        .await
}

/// Mark somebody present. Returns false when they were already marked
/// at this run — the unique constraint makes that a no-op rather than
/// an error, because an admin pressing the button twice is not a
/// mistake worth stopping them for.
pub async fn record_attendance(
    conn: &mut PgConnection,
    user_id: Uuid,
    event_id: Uuid,
    admin_id: Uuid,
) -> sqlx::Result<bool> {
    let done = sqlx::query(
        "INSERT INTO attendances (user_id, event_id, recorded_by_admin_id)
         VALUES ($1, $2, $3)
         ON CONFLICT (user_id, event_id) DO NOTHING",
    )
    .bind(user_id)
    .bind(event_id)
    .bind(admin_id)
    .execute(conn)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Standing, straight from the functions the INSERT policy uses — so
/// what a member is told and what the database will allow cannot
/// disagree.
pub async fn standing(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<(bool, Option<chrono::DateTime<chrono::Utc>>)> {
    sqlx::query_as("SELECT bf_member_is_current($1), bf_member_last_seen($1)")
        .bind(user_id)
        .fetch_one(conn)
        .await
}
