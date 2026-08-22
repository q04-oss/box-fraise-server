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
        .route("/adverts/requests", post(request))
        .route("/advertisers/{id}/ledger", get(ledger))
        .route("/admin/adverts/requests", get(admin_requests))
        .route("/admin/adverts/requests/{id}/accept", post(accept_request))
        .route("/admin/adverts/requests/{id}/delete", post(delete_request))
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

/// A business outlining an advertisement they have. Open to anybody —
/// see 0041 for what bounds it. Nothing appears in an inbox until an
/// admin accepts it.
async fn request(
    State(state): State<AppState>,
    Json(req): Json<NewRequest>,
) -> AppResult<StatusCode> {
    service::request(&state.pool, req).await?;
    Ok(StatusCode::CREATED)
}

/// What a business has spent. No session: they were sent a link, and
/// the id in it is the permission. See 0042.
async fn ledger(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Ledger>> {
    Ok(Json(service::ledger(&state.pool, id).await?))
}

async fn admin_requests(
    AuthedAdmin(_): AuthedAdmin,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<AdminRequest>>> {
    Ok(Json(service::list_requests(&state.pool).await?))
}

async fn accept_request(
    AuthedAdmin(admin_id): AuthedAdmin,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let advert_id = service::accept_request(&state.pool, admin_id, id).await?;
    Ok(Json(serde_json::json!({ "id": advert_id })))
}

async fn delete_request(
    AuthedAdmin(admin_id): AuthedAdmin,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    service::delete_request(&state.pool, admin_id, id).await?;
    Ok(StatusCode::NO_CONTENT)
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
