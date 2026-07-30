use serde_json::json;
use uuid::Uuid;

use crate::{
    audit,
    db::{AdminRlsTransaction, Pool},
    domain::sites::{repository, types::*},
    error::{AppError, AppResult},
};

/// Public list — plain transaction, so the sites SELECT policy
/// resolves to published-only.
pub async fn list_public(pool: &Pool) -> AppResult<Vec<Site>> {
    let mut tx = pool.begin().await?;
    let rows = repository::list_published(&mut tx).await?;
    tx.commit().await?;
    Ok(rows)
}

pub async fn list_admin(pool: &Pool) -> AppResult<Vec<Site>> {
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let rows = repository::list_all(tx.conn()).await?;
    tx.commit().await?;
    Ok(rows)
}

pub async fn create(pool: &Pool, admin_id: Uuid, req: CreateSiteRequest) -> AppResult<Site> {
    let slug = req.slug.trim().to_lowercase();
    if !is_valid_slug(&slug) {
        return Err(AppError::bad_request(
            "slug must be lowercase alphanumeric words separated by single hyphens",
        ));
    }
    let label = req.label.trim();
    if label.is_empty() {
        return Err(AppError::bad_request("label required"));
    }
    if !(-90.0..=90.0).contains(&req.latitude) || !(-180.0..=180.0).contains(&req.longitude) {
        return Err(AppError::bad_request("invalid lat/long"));
    }

    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let site = repository::insert_site(
        tx.conn(),
        &slug,
        label,
        req.blurb
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty()),
        req.latitude,
        req.longitude,
        req.sort_order,
        req.published,
    )
    .await?;
    tx.commit().await?;

    audit::write(
        pool,
        "admin",
        Some(admin_id),
        "site.create",
        Some(&site.id.to_string()),
        json!({ "slug": site.slug, "published": site.published }),
    )
    .await;

    Ok(site)
}

/// Mirrors the `sites_slug_shape` CHECK: lowercase alphanumeric words
/// joined by single hyphens. Validated here too so the operator gets a
/// readable 400 instead of a 500 from a constraint violation.
fn is_valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && !slug.starts_with('-')
        && !slug.ends_with('-')
        && !slug.contains("--")
        && slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}
