use sqlx::PgConnection;
use uuid::Uuid;

use super::types::{AdminTasteLine, TasteLine};

/// The draw. One published line, uniformly at random.
///
/// `ORDER BY random()` sorts the whole published set, which is the
/// right trade at this size — the pool is tens of lines, and the
/// alternatives (a random offset, or a stored sort key) either need a
/// second query for the count or bias the draw as rows are added and
/// removed. Revisit if the pool ever reaches thousands.
///
/// The `status` filter is explicit rather than left to RLS, so the
/// query means the same thing under an admin transaction as under a
/// public one — an admin scanning a strawberry must not start
/// receiving their own drafts.
pub async fn draw_one(conn: &mut PgConnection) -> sqlx::Result<Option<TasteLine>> {
    sqlx::query_as::<_, TasteLine>(
        "SELECT id, body, attribution
           FROM taste_lines
          WHERE status = 'published'
          ORDER BY random()
          LIMIT 1",
    )
    .fetch_optional(conn)
    .await
}

pub async fn list_all(conn: &mut PgConnection) -> sqlx::Result<Vec<AdminTasteLine>> {
    sqlx::query_as::<_, AdminTasteLine>(
        "SELECT id, body, attribution, source, status, created_at
           FROM taste_lines
          ORDER BY created_at DESC",
    )
    .fetch_all(conn)
    .await
}

pub async fn insert(
    conn: &mut PgConnection,
    admin_id: Uuid,
    body: &str,
    attribution: &str,
    source: &str,
    publish: bool,
) -> sqlx::Result<Uuid> {
    // An admin transaction can read this table, so RETURNING is safe
    // here — unlike the blind public write in `submissions`.
    let (id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO taste_lines
             (body, attribution, source, status, published_at, created_by_admin_id)
         VALUES ($1, $2, $3,
                 CASE WHEN $4 THEN 'published' ELSE 'draft' END,
                 CASE WHEN $4 THEN now() ELSE NULL END,
                 $5)
         RETURNING id",
    )
    .bind(body)
    .bind(attribution)
    .bind(source)
    .bind(publish)
    .bind(admin_id)
    .fetch_one(conn)
    .await?;
    Ok(id)
}

/// Publish or withdraw. Returns false if the row is already in that
/// state, so the caller can tell a real change from a no-op.
pub async fn set_published(
    conn: &mut PgConnection,
    id: Uuid,
    published: bool,
) -> sqlx::Result<bool> {
    let done = sqlx::query(
        "UPDATE taste_lines
            SET status = CASE WHEN $2 THEN 'published' ELSE 'draft' END,
                published_at = CASE WHEN $2 THEN COALESCE(published_at, now()) ELSE NULL END
          WHERE id = $1
            AND status <> CASE WHEN $2 THEN 'published' ELSE 'draft' END",
    )
    .bind(id)
    .bind(published)
    .execute(conn)
    .await?;
    Ok(done.rows_affected() == 1)
}

pub async fn delete(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<bool> {
    let done = sqlx::query("DELETE FROM taste_lines WHERE id = $1")
        .bind(id)
        .execute(conn)
        .await?;
    Ok(done.rows_affected() == 1)
}
