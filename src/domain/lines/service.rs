//! The pool the strawberry draws from.
//!
//! Unlike `submissions`, this table is written only by the editor and
//! read by anyone. A published line is the platform's published
//! matter; a draft is the editor still thinking.

use serde_json::json;
use uuid::Uuid;

use super::{
    repository,
    types::{AdminTasteLine, CreateLineRequest, TasteLine},
};
use crate::{
    audit,
    db::{AdminRlsTransaction, Pool},
    error::{AppError, AppResult},
};

/// Mirrors the `taste_lines_body_len` CHECK. A line is read on a phone
/// held up to a sticker, so it stays a line.
const MAX_BODY_CHARS: usize = 120;
const MAX_ATTRIBUTION_CHARS: usize = 80;
const SOURCES: [&str; 3] = ["editor", "business", "member"];

// ── Public ──────────────────────────────────────────────────────────

/// One published line, at random. NotFound when the pool is empty,
/// which the scanner shows as "nothing in the pool yet" rather than an
/// error — an empty pool is a state, not a fault.
pub async fn draw(pool: &Pool) -> AppResult<TasteLine> {
    let mut conn = pool.acquire().await?;
    repository::draw_one(&mut conn)
        .await?
        .ok_or(AppError::NotFound)
}

// ── Admin ───────────────────────────────────────────────────────────

pub async fn list(pool: &Pool) -> AppResult<Vec<AdminTasteLine>> {
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let rows = repository::list_all(tx.conn()).await?;
    tx.commit().await?;
    Ok(rows)
}

pub async fn create(pool: &Pool, admin_id: Uuid, req: CreateLineRequest) -> AppResult<Uuid> {
    let body = clean(&req.body, MAX_BODY_CHARS, "line")?;
    let attribution = clean(&req.attribution, MAX_ATTRIBUTION_CHARS, "attribution")?;
    let source = req.source.unwrap_or_else(|| "editor".into());
    if !SOURCES.contains(&source.as_str()) {
        return Err(AppError::bad_request(
            "source must be editor, business or member",
        ));
    }

    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let id = repository::insert(
        tx.conn(),
        admin_id,
        &body,
        &attribution,
        &source,
        req.publish,
    )
    .await?;
    tx.commit().await?;

    audit::write(
        pool,
        "admin",
        Some(admin_id),
        "line.created",
        Some(&id.to_string()),
        json!({ "source": source, "published": req.publish }),
    )
    .await;
    Ok(id)
}

pub async fn set_published(
    pool: &Pool,
    admin_id: Uuid,
    id: Uuid,
    published: bool,
) -> AppResult<()> {
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let changed = repository::set_published(tx.conn(), id, published).await?;
    tx.commit().await?;
    if !changed {
        // Either it is gone or it was already in that state.
        return Err(AppError::Conflict);
    }
    audit::write(
        pool,
        "admin",
        Some(admin_id),
        if published {
            "line.published"
        } else {
            "line.withdrawn"
        },
        Some(&id.to_string()),
        json!({}),
    )
    .await;
    Ok(())
}

pub async fn delete(pool: &Pool, admin_id: Uuid, id: Uuid) -> AppResult<()> {
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let done = repository::delete(tx.conn(), id).await?;
    tx.commit().await?;
    if !done {
        return Err(AppError::NotFound);
    }
    audit::write(
        pool,
        "admin",
        Some(admin_id),
        "line.deleted",
        Some(&id.to_string()),
        json!({}),
    )
    .await;
    Ok(())
}

// ── Validation ──────────────────────────────────────────────────────

/// Trim and bound. Measured in `chars()` to match Postgres
/// `char_length`, which counts characters rather than bytes.
fn clean(raw: &str, max_chars: usize, what: &str) -> AppResult<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::bad_request(format!("the {what} cannot be empty")));
    }
    if trimmed.chars().count() > max_chars {
        return Err(AppError::bad_request(format!(
            "the {what} exceeds {max_chars} characters"
        )));
    }
    Ok(trimmed.to_owned())
}
