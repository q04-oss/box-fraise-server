use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde_json::json;
use uuid::Uuid;

use crate::{
    app::AppState,
    domain::lines::{service, types::*},
    error::AppResult,
    http::extractors::AuthedAdmin,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/lines/draw", get(draw))
        .route("/admin/lines", get(admin_list).post(admin_create))
        .route("/admin/lines/{id}/publish", post(admin_publish))
        .route("/admin/lines/{id}/withdraw", post(admin_withdraw))
        .route("/admin/lines/{id}/delete", post(admin_delete))
}

// ── Public ──────────────────────────────────────────────────────────

/// One published line, at random. What a strawberry returns.
async fn draw(State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    let line = service::draw(&state.pool).await?;
    let mut headers = HeaderMap::new();
    // Every scan is a fresh draw. A cached response would hand the
    // same line back for the rest of the day, which is the one thing
    // this endpoint must never do.
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok((headers, Json(line)))
}

// ── Admin ───────────────────────────────────────────────────────────

async fn admin_list(
    AuthedAdmin(_admin_id): AuthedAdmin,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<AdminTasteLine>>> {
    Ok(Json(service::list(&state.pool).await?))
}

async fn admin_create(
    AuthedAdmin(admin_id): AuthedAdmin,
    State(state): State<AppState>,
    Json(req): Json<CreateLineRequest>,
) -> AppResult<(StatusCode, Json<serde_json::Value>)> {
    let id = service::create(&state.pool, admin_id, req).await?;
    Ok((StatusCode::CREATED, Json(json!({ "id": id }))))
}

async fn admin_publish(
    AuthedAdmin(admin_id): AuthedAdmin,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    service::set_published(&state.pool, admin_id, id, true).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn admin_withdraw(
    AuthedAdmin(admin_id): AuthedAdmin,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    service::set_published(&state.pool, admin_id, id, false).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn admin_delete(
    AuthedAdmin(admin_id): AuthedAdmin,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    service::delete(&state.pool, admin_id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
