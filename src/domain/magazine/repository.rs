use sqlx::PgConnection;
use uuid::Uuid;

use super::types::PendingMagazineSubmission;

/// The open write.
///
/// The id is generated here rather than read back with RETURNING.
/// Postgres applies SELECT policies to the row an INSERT returns, and
/// this table has no public SELECT policy at all, so RETURNING would
/// fail with 42501 even though the insert is permitted. Same shape as
/// submissions — see 0018 and CLAUDE.md.
pub async fn insert(conn: &mut PgConnection, body: &str) -> sqlx::Result<Uuid> {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO magazine_submissions (id, body, status) VALUES ($1, $2, 'pending')")
        .bind(id)
        .bind(body)
        .execute(conn)
        .await?;
    Ok(id)
}

pub async fn pending_count(conn: &mut PgConnection) -> sqlx::Result<i32> {
    sqlx::query_scalar::<_, i32>("SELECT bf_pending_magazine_count()")
        .fetch_one(conn)
        .await
}

pub async fn list_pending(
    conn: &mut PgConnection,
) -> sqlx::Result<Vec<PendingMagazineSubmission>> {
    sqlx::query_as::<_, PendingMagazineSubmission>(
        "SELECT id, body, submitted_at
           FROM magazine_submissions
          WHERE status = 'pending'
          ORDER BY submitted_at ASC",
    )
    .fetch_all(conn)
    .await
}

/// Kept for an edition. Race-close on status so two admins keeping the
/// same piece at once means exactly one UPDATE touches a row.
pub async fn keep(conn: &mut PgConnection, admin_id: Uuid, id: Uuid) -> sqlx::Result<bool> {
    let done = sqlx::query(
        "UPDATE magazine_submissions
            SET status = 'kept', reviewed_at = now(), reviewed_by_admin_id = $1
          WHERE id = $2 AND status = 'pending'",
    )
    .bind(admin_id)
    .bind(id)
    .execute(conn)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Rejection deletes. Keeping anonymous writing nobody will print is a
/// liability with no upside.
pub async fn delete(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<bool> {
    let done = sqlx::query("DELETE FROM magazine_submissions WHERE id = $1")
        .bind(id)
        .execute(conn)
        .await?;
    Ok(done.rows_affected() == 1)
}
