use sqlx::PgConnection;

use super::types::Site;

const SITE_COLUMNS: &str = "id, slug, label, blurb, latitude, longitude,
                            sort_order, published, created_at";

/// Public list: published sites only. RLS enforces the same thing;
/// the WHERE clause keeps it obvious at the call site.
pub async fn list_published(conn: &mut PgConnection) -> sqlx::Result<Vec<Site>> {
    sqlx::query_as::<_, Site>(&format!(
        "SELECT {SITE_COLUMNS}
           FROM sites
          WHERE published = true
          ORDER BY sort_order DESC, label ASC"
    ))
    .fetch_all(conn)
    .await
}

/// Admin list: drafts included. Requires an AdminRlsTransaction —
/// without `app.is_admin` the SELECT policy filters unpublished rows
/// and this silently behaves like `list_published`.
pub async fn list_all(conn: &mut PgConnection) -> sqlx::Result<Vec<Site>> {
    sqlx::query_as::<_, Site>(&format!(
        "SELECT {SITE_COLUMNS}
           FROM sites
          ORDER BY sort_order DESC, label ASC"
    ))
    .fetch_all(conn)
    .await
}

/// Matches `insert_sticker`'s shape: a flat column list rather than a
/// struct, so the repository layer stays SQL-only and does not depend
/// on the request types.
#[allow(clippy::too_many_arguments)]
pub async fn insert_site(
    conn: &mut PgConnection,
    slug: &str,
    label: &str,
    blurb: Option<&str>,
    latitude: f64,
    longitude: f64,
    sort_order: i32,
    published: bool,
) -> sqlx::Result<Site> {
    sqlx::query_as::<_, Site>(&format!(
        "INSERT INTO sites (slug, label, blurb, latitude, longitude, sort_order, published)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING {SITE_COLUMNS}"
    ))
    .bind(slug)
    .bind(label)
    .bind(blurb)
    .bind(latitude)
    .bind(longitude)
    .bind(sort_order)
    .bind(published)
    .fetch_one(conn)
    .await
}
