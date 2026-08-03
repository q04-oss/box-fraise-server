use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    app::AppState,
    domain::pairings::{service, types::*},
    error::AppResult,
    http::extractors::AuthedUser,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/pairings", get(list))
        .route("/pairings/nonce", post(nonce))
        .route("/pairings/claim", post(claim))
        .route("/pairings/authorized", get(authorized))
        .route("/pairings/{id}/decision", post(decision))
        .route("/pairings/{id}/block", post(block))
        .route("/me/display-name", post(set_display_name))
}

async fn list(
    AuthedUser(me): AuthedUser,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<PairingView>>> {
    Ok(Json(service::list(&state.pool, me).await?))
}

async fn nonce(
    AuthedUser(me): AuthedUser,
    State(state): State<AppState>,
) -> AppResult<Json<PairingNonceResponse>> {
    Ok(Json(service::issue_nonce(&state.pool, me).await?))
}

async fn claim(
    AuthedUser(me): AuthedUser,
    State(state): State<AppState>,
    Json(req): Json<ClaimRequest>,
) -> AppResult<Json<ClaimResponse>> {
    let (cooling, window) = (state.cfg.pairing_cooling, state.cfg.pairing_window);
    Ok(Json(
        service::claim(&state.pool, me, cooling, window, req).await?,
    ))
}

async fn decision(
    AuthedUser(me): AuthedUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<DecisionRequest>,
) -> AppResult<Json<PairingView>> {
    Ok(Json(service::decide(&state.pool, me, id, req).await?))
}

async fn block(
    AuthedUser(me): AuthedUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    service::block(&state.pool, me, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct AuthorizedQuery {
    pub peer: Uuid,
}

/// Asked by `box-fraise-chat` before it lets a message through, and
/// before it serves a prekey bundle. Answers only about the caller's
/// own pairings, so it cannot be used to probe anyone else's.
async fn authorized(
    AuthedUser(me): AuthedUser,
    State(state): State<AppState>,
    Query(q): Query<AuthorizedQuery>,
) -> AppResult<Json<AuthorizedResponse>> {
    Ok(Json(service::authorized(&state.pool, me, q.peer).await?))
}

async fn set_display_name(
    AuthedUser(me): AuthedUser,
    State(state): State<AppState>,
    Json(req): Json<SetDisplayNameRequest>,
) -> AppResult<StatusCode> {
    service::set_display_name(&state.pool, me, req).await?;
    Ok(StatusCode::NO_CONTENT)
}
