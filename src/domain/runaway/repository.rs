use sqlx::PgConnection;
use uuid::Uuid;

use super::types::{PendingAnswer, PublishedAnswer};

/// The open write. `status` is hardcoded rather than taken from the
/// request, and the id is generated here rather than read back with
/// RETURNING — the public role has no SELECT on a pending row, so
/// RETURNING would fail with 42501 the way it does on submissions.
pub async fn insert(conn: &mut PgConnection, body: &str) -> sqlx::Result<Uuid> {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO runaway_answers (id, body, status) VALUES ($1, $2, 'pending')")
        .bind(id)
        .bind(body)
        .execute(conn)
        .await?;
    Ok(id)
}

/// Global pending count, through the SECURITY DEFINER function — a
/// public transaction cannot see pending rows to count them itself.
pub async fn pending_count(conn: &mut PgConnection) -> sqlx::Result<i32> {
    sqlx::query_scalar::<_, i32>("SELECT bf_pending_runaway_count()")
        .fetch_one(conn)
        .await
}

/// What /runaway shows. The status filter is explicit rather than left
/// to RLS, so the query means the same thing under an admin transaction
/// as under a public one.
pub async fn list_published(
    conn: &mut PgConnection,
    limit: i64,
) -> sqlx::Result<Vec<PublishedAnswer>> {
    sqlx::query_as::<_, PublishedAnswer>(
        "SELECT id, body, reviewed_at AS published_at
           FROM runaway_answers
          WHERE status = 'accepted'
          ORDER BY reviewed_at DESC
          LIMIT $1",
    )
    .bind(limit)
    .fetch_all(conn)
    .await
}

pub async fn list_pending(conn: &mut PgConnection) -> sqlx::Result<Vec<PendingAnswer>> {
    sqlx::query_as::<_, PendingAnswer>(
        "SELECT id, body, submitted_at
           FROM runaway_answers
          WHERE status = 'pending'
          ORDER BY submitted_at ASC",
    )
    .fetch_all(conn)
    .await
}

/// Race-close on status, so two admins accepting at once means exactly
/// one UPDATE touches a row.
pub async fn accept(conn: &mut PgConnection, admin_id: Uuid, id: Uuid) -> sqlx::Result<bool> {
    let done = sqlx::query(
        "UPDATE runaway_answers
            SET status = 'accepted', reviewed_at = now(), reviewed_by_admin_id = $1
          WHERE id = $2 AND status = 'pending'",
    )
    .bind(admin_id)
    .bind(id)
    .execute(conn)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Rejection deletes. Keeping anonymous writing nobody will publish is
/// a liability with no upside.
pub async fn delete(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<bool> {
    let done = sqlx::query("DELETE FROM runaway_answers WHERE id = $1")
        .bind(id)
        .execute(conn)
        .await?;
    Ok(done.rows_affected() == 1)
}
