use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use uuid::Uuid;

use crate::{
    app::AppState,
    domain::adverts::{service, types::*},
    error::AppResult,
    http::extractors::{AuthedAdmin, AuthedRunner},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/running/inbox", get(inbox))
        .route("/running/inbox/{id}/open", post(open))
        .route("/admin/adverts", get(admin_list).post(create))
        .route("/admin/adverts/{id}/close", post(close))
        .route("/admin/adverts/owed", get(owed))
        .route("/admin/adverts/pay", post(pay))
}

/// A runner's inbox. There is no signed-out version: an advert is
/// addressed to people who signed up, and what is still unspent on it is
/// between the advertiser and the platform.
async fn inbox(
    AuthedRunner(runner_id): AuthedRunner,
    State(state): State<AppState>,
) -> AppResult<Json<Inbox>> {
    Ok(Json(service::inbox(&state.pool, runner_id).await?))
}

/// Choose to open one. This is the only path that ever returns `body`.
async fn open(
    AuthedRunner(runner_id): AuthedRunner,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Opened>> {
    Ok(Json(service::open(&state.pool, runner_id, id).await?))
}

async fn admin_list(
    AuthedAdmin(_): AuthedAdmin,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<AdminAdvert>>> {
    Ok(Json(service::list_all(&state.pool).await?))
}

async fn create(
    AuthedAdmin(admin_id): AuthedAdmin,
    State(state): State<AppState>,
    Json(req): Json<NewAdvert>,
) -> AppResult<(StatusCode, Json<serde_json::Value>)> {
    let id = service::create(&state.pool, admin_id, req).await?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": id }))))
}

async fn close(
    AuthedAdmin(admin_id): AuthedAdmin,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    service::close(&state.pool, admin_id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn owed(
    AuthedAdmin(_): AuthedAdmin,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<Owed>>> {
    Ok(Json(service::owed(&state.pool).await?))
}

async fn pay(
    AuthedAdmin(admin_id): AuthedAdmin,
    State(state): State<AppState>,
    Json(req): Json<PayRequest>,
) -> AppResult<Json<Paid>> {
    Ok(Json(service::pay(&state.pool, admin_id, req).await?))
}
