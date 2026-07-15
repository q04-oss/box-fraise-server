use sqlx::PgConnection;

use super::types::Business;

/// Public listing: published rows, sort_order descending, then name.
pub async fn list_published(conn: &mut PgConnection) -> sqlx::Result<Vec<Business>> {
    sqlx::query_as::<_, Business>(
        "SELECT id, name, description, website, location, slug, sort_order, created_at
           FROM businesses
          WHERE published = true
          ORDER BY sort_order DESC, name ASC",
    )
    .fetch_all(conn)
    .await
}

/// Single published row by URL slug. RLS also gates the read to
/// published rows, but the WHERE clause keeps the semantics obvious
/// at the call site.
pub async fn get_by_slug(conn: &mut PgConnection, slug: &str) -> sqlx::Result<Option<Business>> {
    sqlx::query_as::<_, Business>(
        "SELECT id, name, description, website, location, slug, sort_order, created_at
           FROM businesses
          WHERE slug = $1 AND published = true",
    )
    .bind(slug)
    .fetch_optional(conn)
    .await
}
