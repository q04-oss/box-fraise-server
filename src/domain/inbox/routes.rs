use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use uuid::Uuid;

use crate::{
    app::AppState,
    domain::inbox::{service, types::*},
    error::AppResult,
    http::extractors::{AuthedAdmin, AuthedUser},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/inbox", get(inbox))
        .route("/inbox/{id}/accept", post(accept))
        .route("/admin/offers", get(admin_list))
        .route("/admin/offers", post(create))
        .route("/admin/offers/{id}/close", post(close))
        .route("/admin/offers/owed", get(owed))
        .route("/admin/offers/pay", post(pay))
}

/// A member's inbox. There is no signed-out version of this: an offer
/// is addressed to members, and the budget still on it is between the
/// business and the platform.
async fn inbox(
    AuthedUser(user_id): AuthedUser,
    State(state): State<AppState>,
) -> AppResult<Json<Inbox>> {
    Ok(Json(service::inbox(&state.pool, user_id).await?))
}

async fn accept(
    AuthedUser(user_id): AuthedUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Accepted>> {
    Ok(Json(service::accept(&state.pool, user_id, id).await?))
}

async fn admin_list(
    AuthedAdmin(_): AuthedAdmin,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<AdminOffer>>> {
    Ok(Json(service::list_all(&state.pool).await?))
}

async fn create(
    AuthedAdmin(admin_id): AuthedAdmin,
    State(state): State<AppState>,
    Json(req): Json<NewOffer>,
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

/// Who to pay at the next run.
async fn owed(
    AuthedAdmin(_): AuthedAdmin,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<Owed>>> {
    Ok(Json(service::owed(&state.pool).await?))
}

/// Record that money was handed over. An admin types a member's number
/// while looking at them, the same as marking attendance.
async fn pay(
    AuthedAdmin(admin_id): AuthedAdmin,
    State(state): State<AppState>,
    Json(req): Json<PayRequest>,
) -> AppResult<Json<Paid>> {
    Ok(Json(service::pay(&state.pool, admin_id, req).await?))
}
