use axum::{extract::State, routing::post, Json, Router};

use crate::{
    app::AppState,
    domain::admin::service::{self, LoginRequest, LoginResponse},
    error::AppResult,
};

pub fn router() -> Router<AppState> {
    Router::new().route("/admin/login", post(login_handler))
}

async fn login_handler(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> AppResult<Json<LoginResponse>> {
    Ok(Json(
        service::login(&state.pool, state.cfg.admin_session_ttl, req).await?,
    ))
}
