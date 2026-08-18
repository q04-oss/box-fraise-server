use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, put},
    Json, Router,
};
use uuid::Uuid;

use crate::{
    app::AppState,
    domain::messages::{service, types::*},
    error::AppResult,
    http::extractors::AuthedUser,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/members/me/key", put(publish_key))
        .route("/pairings/{id}/key", get(peer_key))
        .route("/pairings/{id}/messages", get(list).post(send))
}

/// Publish the public half of an ECDH keypair made in the browser.
async fn publish_key(
    AuthedUser(user_id): AuthedUser,
    State(state): State<AppState>,
    Json(req): Json<PublishKeyRequest>,
) -> AppResult<StatusCode> {
    service::publish_key(&state.pool, user_id, req).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// The other person's key, so a shared secret can be derived. Returns
/// nothing unless the channel is open — see 0026.
async fn peer_key(
    AuthedUser(user_id): AuthedUser,
    State(state): State<AppState>,
    Path(peer_id): Path<Uuid>,
) -> AppResult<Json<PeerKey>> {
    Ok(Json(
        service::peer_key(&state.pool, user_id, peer_id).await?,
    ))
}

async fn send(
    AuthedUser(user_id): AuthedUser,
    State(state): State<AppState>,
    Path(pairing_id): Path<Uuid>,
    Json(req): Json<SendMessageRequest>,
) -> AppResult<(StatusCode, Json<Message>)> {
    Ok((
        StatusCode::CREATED,
        Json(service::send(&state.pool, user_id, pairing_id, req).await?),
    ))
}

async fn list(
    AuthedUser(user_id): AuthedUser,
    State(state): State<AppState>,
    Path(pairing_id): Path<Uuid>,
) -> AppResult<Json<Vec<Message>>> {
    Ok(Json(service::list(&state.pool, user_id, pairing_id).await?))
}
