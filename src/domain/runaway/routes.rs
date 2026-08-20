use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use uuid::Uuid;

use crate::{
    app::AppState,
    domain::runaway::{service, types::*},
    error::AppResult,
    http::extractors::AuthedAdmin,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/runaway/answers", get(published).post(submit))
        .route("/admin/runaway/answers", get(admin_pending))
        .route("/admin/runaway/answers/{id}/accept", post(admin_accept))
        .route("/admin/runaway/answers/{id}/reject", post(admin_reject))
}

/// Accepted answers, for /runaway. Public.
async fn published(State(state): State<AppState>) -> AppResult<Json<Vec<PublishedAnswer>>> {
    Ok(Json(service::list_published(&state.pool).await?))
}

/// The one open write on the platform. See service and 0032.
async fn submit(
    State(state): State<AppState>,
    Json(upload): Json<AnswerUpload>,
) -> AppResult<Json<AnswerReceived>> {
    Ok(Json(service::submit(&state.pool, upload).await?))
}

async fn admin_pending(
    AuthedAdmin(_): AuthedAdmin,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<PendingAnswer>>> {
    Ok(Json(service::list_pending(&state.pool).await?))
}

async fn admin_accept(
    AuthedAdmin(admin_id): AuthedAdmin,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    service::accept(&state.pool, admin_id, id).await?;
    Ok(Json(serde_json::json!({ "published": true })))
}

async fn admin_reject(
    AuthedAdmin(admin_id): AuthedAdmin,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    service::reject(&state.pool, admin_id, id).await?;
    Ok(Json(serde_json::json!({ "rejected": true })))
}
