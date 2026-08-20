use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use uuid::Uuid;

use crate::{
    app::AppState,
    domain::magazine::{service, types::*},
    error::AppResult,
    http::extractors::AuthedAdmin,
};

pub fn router() -> Router<AppState> {
    Router::new()
        // No GET counterpart on purpose: nothing in this table is ever
        // served to the web. See 0033.
        .route("/magazine/submissions", post(submit))
        .route("/admin/magazine/submissions", get(admin_pending))
        .route("/admin/magazine/submissions/{id}/keep", post(admin_keep))
        .route("/admin/magazine/submissions/{id}/reject", post(admin_reject))
}

async fn submit(
    State(state): State<AppState>,
    Json(upload): Json<MagazineUpload>,
) -> AppResult<Json<MagazineReceived>> {
    Ok(Json(service::submit(&state.pool, upload).await?))
}

async fn admin_pending(
    AuthedAdmin(_): AuthedAdmin,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<PendingMagazineSubmission>>> {
    Ok(Json(service::list_pending(&state.pool).await?))
}

async fn admin_keep(
    AuthedAdmin(admin_id): AuthedAdmin,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    service::keep(&state.pool, admin_id, id).await?;
    Ok(Json(serde_json::json!({ "kept": true })))
}

async fn admin_reject(
    AuthedAdmin(admin_id): AuthedAdmin,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    service::reject(&state.pool, admin_id, id).await?;
    Ok(Json(serde_json::json!({ "rejected": true })))
}
