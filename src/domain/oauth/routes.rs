use axum::{
    extract::State,
    http::HeaderMap,
    routing::{get, post},
    Json, Router,
};

use crate::{
    app::AppState,
    domain::oauth::{
        service,
        types::{Jwks, TokenRequest, TokenResponse, UserInfo},
    },
    error::{AppError, AppResult},
    http::extractors::AuthedUser,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/oauth/token", post(token_handler))
        .route("/oauth/jwks", get(jwks_handler))
        .route("/oauth/userinfo", get(userinfo_handler))
}

async fn token_handler(
    AuthedUser(user_id): AuthedUser,
    State(state): State<AppState>,
    Json(req): Json<TokenRequest>,
) -> AppResult<Json<TokenResponse>> {
    Ok(Json(service::issue_token(&state.pool, user_id, req).await?))
}

async fn jwks_handler() -> Json<Jwks> {
    Json(service::jwks())
}

async fn userinfo_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> AppResult<Json<UserInfo>> {
    let auth = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or(AppError::Unauthorized)?;
    let token = auth
        .strip_prefix("Bearer ")
        .or_else(|| auth.strip_prefix("bearer "))
        .ok_or(AppError::Unauthorized)?
        .trim();
    Ok(Json(service::userinfo(&state.pool, token).await?))
}
