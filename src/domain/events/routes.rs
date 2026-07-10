use axum::{
    extract::{Path, State},
    routing::get,
    Json, Router,
};
use uuid::Uuid;

use crate::{
    app::AppState,
    domain::events::{service, types::*},
    error::AppResult,
    http::extractors::AuthedAdmin,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/events", get(public_list))
        .route("/events/{id}", get(public_get))
        .route("/questions", get(public_questions))
        .route("/admin/events", get(admin_list).post(admin_create))
        .route(
            "/admin/events/{id}/verified-count",
            get(admin_verified_count),
        )
}

async fn public_questions(State(state): State<AppState>) -> AppResult<Json<Vec<EventQuestions>>> {
    Ok(Json(service::list_all_questions(&state.pool).await?))
}

async fn public_list(State(state): State<AppState>) -> AppResult<Json<Vec<EventSummary>>> {
    Ok(Json(service::list_public(&state.pool).await?))
}

async fn public_get(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<EventSummary>> {
    Ok(Json(service::get_public(&state.pool, id).await?))
}

async fn admin_list(
    _admin: AuthedAdmin,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<EventSummary>>> {
    Ok(Json(service::list_admin(&state.pool).await?))
}

async fn admin_create(
    AuthedAdmin(admin_id): AuthedAdmin,
    State(state): State<AppState>,
    Json(req): Json<CreateEventRequest>,
) -> AppResult<Json<EventSummary>> {
    Ok(Json(service::create(&state.pool, admin_id, req).await?))
}

async fn admin_verified_count(
    _admin: AuthedAdmin,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<VerifiedCountResponse>> {
    Ok(Json(service::verified_count(&state.pool, id).await?))
}
