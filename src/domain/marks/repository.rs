use sqlx::PgConnection;
use uuid::Uuid;

use super::types::{AdminMark, Billboard, PublicMark};

/// What the scanner asks for on load. Published only, in the order an
/// editor chose — first match wins, so order is meaningful.
pub async fn list_published(conn: &mut PgConnection) -> sqlx::Result<Vec<PublicMark>> {
    sqlx::query_as::<_, PublicMark>(
        "SELECT id, label, act, target
           FROM marks
          WHERE published
          ORDER BY sort_order ASC, created_at ASC",
    )
    .fetch_all(conn)
    .await
}

pub async fn image(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<Option<(Vec<u8>, String)>> {
    sqlx::query_as("SELECT image_bytes, content_type FROM marks WHERE id = $1 AND published")
        .bind(id)
        .fetch_optional(conn)
        .await
}

/// What the game draws along the street.
pub async fn billboards(conn: &mut PgConnection) -> sqlx::Result<Vec<Billboard>> {
    sqlx::query_as::<_, Billboard>(
        "SELECT id, label
           FROM marks
          WHERE published AND in_game
          ORDER BY sort_order ASC, created_at ASC",
    )
    .fetch_all(conn)
    .await
}

/// The editor's list, including unpublished ones.
pub async fn list_all(conn: &mut PgConnection) -> sqlx::Result<Vec<AdminMark>> {
    sqlx::query_as::<_, AdminMark>(
        "SELECT m.id, m.label, m.act, m.target, m.published, m.in_game, b.name AS business_name
           FROM marks m
           LEFT JOIN businesses b ON b.id = m.business_id
          ORDER BY m.sort_order ASC, m.created_at ASC",
    )
    .fetch_all(conn)
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn insert(
    conn: &mut PgConnection,
    label: &str,
    bytes: &[u8],
    content_type: &str,
    act: &str,
    target: Option<&str>,
    business_id: Option<Uuid>,
    in_game: bool,
    admin_id: Uuid,
) -> sqlx::Result<Uuid> {
    sqlx::query_scalar(
        "INSERT INTO marks
             (label, image_bytes, content_type, act, target, business_id,
              in_game, created_by_admin_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         RETURNING id",
    )
    .bind(label)
    .bind(bytes)
    .bind(content_type)
    .bind(act)
    .bind(target)
    .bind(business_id)
    .bind(in_game)
    .bind(admin_id)
    .fetch_one(conn)
    .await
}

/// Taking a mark down is not deleting it: the poster is still on a wall
/// somewhere, and an editor may want it back. Unpublishing stops the
/// scanner offering it without losing what it was.
pub async fn set_published(
    conn: &mut PgConnection,
    id: Uuid,
    published: bool,
) -> sqlx::Result<bool> {
    let done = sqlx::query("UPDATE marks SET published = $1 WHERE id = $2")
        .bind(published)
        .bind(id)
        .execute(conn)
        .await?;
    Ok(done.rows_affected() == 1)
}

pub async fn delete(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<bool> {
    let done = sqlx::query("DELETE FROM marks WHERE id = $1")
        .bind(id)
        .execute(conn)
        .await?;
    Ok(done.rows_affected() == 1)
}
