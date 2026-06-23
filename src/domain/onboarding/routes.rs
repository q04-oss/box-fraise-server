use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};

use crate::{
    app::AppState,
    domain::onboarding::{service, types::*},
    error::AppResult,
    http::extractors::{AuthedAdmin, AuthedUser},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/onboard/register", post(register_handler))
        .route("/onboard/challenge", post(challenge_handler))
        .route("/admin/verify", post(verify_handler))
        .route("/me", get(me_handler))
}

async fn register_handler(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> AppResult<Json<RegisterResponse>> {
    Ok(Json(service::register(&state.pool, req).await?))
}

async fn challenge_handler(
    AuthedUser(user_id): AuthedUser,
    State(state): State<AppState>,
) -> AppResult<Json<ChallengeResponse>> {
    Ok(Json(
        service::issue_challenge(&state.pool, state.cfg.challenge_ttl, user_id).await?,
    ))
}

async fn verify_handler(
    AuthedAdmin(admin_id): AuthedAdmin,
    State(state): State<AppState>,
    Json(req): Json<VerifyRequest>,
) -> AppResult<Json<VerifyResponse>> {
    Ok(Json(service::verify(&state.pool, admin_id, req).await?))
}

async fn me_handler(
    AuthedUser(user_id): AuthedUser,
    State(state): State<AppState>,
) -> AppResult<Json<MeResponse>> {
    Ok(Json(service::me(&state.pool, user_id).await?))
}
