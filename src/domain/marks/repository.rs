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
        "SELECT m.id, m.label, m.act, m.target, m.published, m.in_game,
                b.name AS business_name,
                COALESCE((SELECT SUM(i.seen) FROM mark_impressions i
                           WHERE i.mark_id = m.id), 0)::bigint AS impressions
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

/// Add to today's counter for a mark.
///
/// Through the SECURITY DEFINER function from 0037 rather than an
/// upsert here. `ON CONFLICT DO UPDATE` has to see the conflicting row
/// to add to it, and SELECT on this table is admin-only — a business's
/// figures are not published. `bf_app` has no INSERT or UPDATE on it at
/// all, which is tighter than a policy: it can add to a counter and
/// cannot write an arbitrary row.
pub async fn record_impression(
    conn: &mut PgConnection,
    mark_id: Uuid,
    seen: i32,
) -> sqlx::Result<()> {
    sqlx::query("SELECT bf_record_impression($1, $2)")
        .bind(mark_id)
        .bind(seen)
        .execute(conn)
        .await?;
    Ok(())
}

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
