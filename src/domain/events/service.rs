use serde_json::json;
use uuid::Uuid;

use crate::{
    audit,
    db::{AdminRlsTransaction, Pool},
    domain::events::{repository, types::*},
    error::{AppError, AppResult},
};

/// Public list — plain transaction (no admin GUC set), so the events
/// SELECT policy resolves to "published only."
pub async fn list_public(pool: &Pool) -> AppResult<Vec<EventSummary>> {
    let mut tx = pool.begin().await?;
    let events = repository::list_upcoming(&mut tx).await?;
    tx.commit().await?;
    Ok(events.into_iter().map(EventSummary::from).collect())
}

pub async fn get_public(pool: &Pool, id: Uuid) -> AppResult<EventSummary> {
    let mut tx = pool.begin().await?;
    let event = repository::get_by_id(&mut tx, id).await?;
    tx.commit().await?;
    // RLS already filters out unpublished — a hit here is by definition
    // published — but be defensive in case the policy is loosened later.
    let event = event.ok_or(AppError::NotFound)?;
    if !event.published {
        return Err(AppError::NotFound);
    }
    Ok(event.into())
}

pub async fn list_admin(pool: &Pool) -> AppResult<Vec<EventSummary>> {
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let events = repository::list_upcoming(tx.conn()).await?;
    tx.commit().await?;
    Ok(events.into_iter().map(EventSummary::from).collect())
}

pub async fn create(
    pool: &Pool,
    admin_id: Uuid,
    req: CreateEventRequest,
) -> AppResult<EventSummary> {
    if req.ends_at <= req.starts_at {
        return Err(AppError::bad_request("ends_at must be after starts_at"));
    }
    if req.name.trim().is_empty() {
        return Err(AppError::bad_request("name required"));
    }
    if req.host_name.trim().is_empty() {
        return Err(AppError::bad_request("host_name required"));
    }
    if req.address.trim().is_empty() {
        return Err(AppError::bad_request("address required"));
    }
    if !(-90.0..=90.0).contains(&req.latitude) || !(-180.0..=180.0).contains(&req.longitude) {
        return Err(AppError::bad_request("invalid lat/long"));
    }

    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let event = repository::insert(
        tx.conn(),
        admin_id,
        req.name.trim(),
        req.description.as_deref().map(str::trim),
        &req.questions,
        req.poster_url.as_deref().map(str::trim),
        req.host_name.trim(),
        req.latitude,
        req.longitude,
        req.address.trim(),
        req.starts_at,
        req.ends_at,
        req.published,
    )
    .await?;
    tx.commit().await?;

    audit::write(
        pool,
        "admin",
        Some(admin_id),
        "event.create",
        Some(&event.id.to_string()),
        json!({ "name": event.name, "published": event.published }),
    )
    .await;

    Ok(event.into())
}

/// Public archive of all discussion questions across events.
pub async fn list_all_questions(pool: &Pool) -> AppResult<Vec<EventQuestions>> {
    let mut tx = pool.begin().await?;
    let rows = repository::list_all_questions(&mut tx).await?;
    tx.commit().await?;
    Ok(rows)
}

pub async fn verified_count(pool: &Pool, event_id: Uuid) -> AppResult<VerifiedCountResponse> {
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let count = repository::verified_count(tx.conn(), event_id).await?;
    tx.commit().await?;
    Ok(VerifiedCountResponse {
        event_id,
        verified_count: count,
    })
}
