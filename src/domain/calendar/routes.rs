use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use uuid::Uuid;

use crate::{
    app::AppState,
    domain::calendar::{service, types::*},
    error::AppResult,
    http::extractors::{AuthedAdmin, AuthedUser},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/members/calendar", get(mine))
        .route("/admin/shifts", post(publish))
        .route("/admin/shifts/{id}/cancel", post(cancel))
        .route("/admin/employments", post(record))
        .route("/admin/employments/end", post(end))
}

/// A member's own calendar — their shifts and the runs, in time order.
async fn mine(
    AuthedUser(user_id): AuthedUser,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<CalendarEntry>>> {
    Ok(Json(service::mine(&state.pool, user_id).await?))
}

/// Publish a shift. Admin-only until businesses can sign in.
async fn publish(
    AuthedAdmin(admin_id): AuthedAdmin,
    State(state): State<AppState>,
    Json(req): Json<PublishShiftRequest>,
) -> AppResult<Json<PublishedShift>> {
    Ok(Json(service::publish_shift(&state.pool, admin_id, req).await?))
}

/// Take one away. Never an edit — see `service::cancel_shift`.
async fn cancel(
    AuthedAdmin(admin_id): AuthedAdmin,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    service::cancel_shift(&state.pool, admin_id, id).await?;
    Ok(Json(serde_json::json!({ "cancelled": true })))
}

async fn record(
    AuthedAdmin(admin_id): AuthedAdmin,
    State(state): State<AppState>,
    Json(req): Json<RecordEmploymentRequest>,
) -> AppResult<Json<EmploymentRecorded>> {
    Ok(Json(
        service::record_employment(&state.pool, admin_id, req).await?,
    ))
}

async fn end(
    AuthedAdmin(admin_id): AuthedAdmin,
    State(state): State<AppState>,
    Json(req): Json<RecordEmploymentRequest>,
) -> AppResult<Json<serde_json::Value>> {
    service::end_employment(&state.pool, admin_id, req).await?;
    Ok(Json(serde_json::json!({ "ended": true })))
}
