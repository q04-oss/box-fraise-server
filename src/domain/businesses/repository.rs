use sqlx::PgConnection;

use super::types::Business;

/// Public listing: published rows, sort_order descending, then name.
pub async fn list_published(conn: &mut PgConnection) -> sqlx::Result<Vec<Business>> {
    sqlx::query_as::<_, Business>(
        "SELECT id, name, description, website, sort_order, created_at
           FROM businesses
          WHERE published = true
          ORDER BY sort_order DESC, name ASC",
    )
    .fetch_all(conn)
    .await
}
