use axum::{extract::State, http::StatusCode, routing::post, Json, Router};

use crate::{
    app::AppState,
    domain::members::{service, types::*},
    error::AppResult,
    http::extractors::AuthedAdmin,
};

pub fn router() -> Router<AppState> {
    Router::new().route("/admin/members", post(create))
}

/// Sign somebody up at the run club. Admin-only, because the whole
/// point is that a person did the verifying.
async fn create(
    AuthedAdmin(admin_id): AuthedAdmin,
    State(state): State<AppState>,
    Json(req): Json<CreateMemberRequest>,
) -> AppResult<(StatusCode, Json<CreatedMember>)> {
    Ok((
        StatusCode::CREATED,
        Json(service::create(&state.pool, admin_id, req).await?),
    ))
}
