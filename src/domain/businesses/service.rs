use crate::{
    db::Pool,
    domain::businesses::{repository, types::Business},
    error::{AppError, AppResult},
};

/// Public list. RLS on the businesses table gates it to published
/// rows under a non-admin transaction — no explicit filter needed
/// here beyond what the SELECT policy enforces.
pub async fn list_public(pool: &Pool) -> AppResult<Vec<Business>> {
    let mut tx = pool.begin().await?;
    let rows = repository::list_published(&mut tx).await?;
    tx.commit().await?;
    Ok(rows)
}

/// Single business by slug. 404 if no published row matches.
pub async fn get_by_slug(pool: &Pool, slug: &str) -> AppResult<Business> {
    let mut tx = pool.begin().await?;
    let row = repository::get_by_slug(&mut tx, slug).await?;
    tx.commit().await?;
    row.ok_or(AppError::NotFound)
}
