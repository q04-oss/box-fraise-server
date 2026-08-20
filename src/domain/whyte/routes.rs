use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use uuid::Uuid;

use crate::{
    app::AppState,
    domain::whyte::{service, types::*},
    error::AppResult,
    http::extractors::AuthedAdmin,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/whyte/scores", get(board).post(submit))
        .route("/whyte/streams", get(streams))
        .route("/whyte/stream", get(broadcast))
        .route("/whyte/watch/{id}", get(watch))
        .route("/admin/whyte/scores/{id}/delete", post(remove))
}

async fn board(State(state): State<AppState>) -> AppResult<Json<Vec<BoardRow>>> {
    Ok(Json(service::board(&state.pool).await?))
}

/// Public. Three letters and a distance — see 0035 for why this one is
/// not behind a membership.
async fn submit(
    State(state): State<AppState>,
    Json(upload): Json<ScoreUpload>,
) -> AppResult<Json<ScorePosted>> {
    Ok(Json(service::post(&state.pool, upload).await?))
}

async fn remove(
    AuthedAdmin(_): AuthedAdmin,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    service::delete(&state.pool, id).await?;
    Ok(Json(serde_json::json!({ "deleted": true })))
}

// ── Live ────────────────────────────────────────────────────────────
// Two sockets: one a player pushes frames into, one a viewer reads from.
// Nothing is stored — see domain::whyte::streams for why the frames are
// game state rather than video.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use std::sync::atomic::Ordering;

/// Who is live now.
pub async fn streams(State(state): State<AppState>) -> Json<Vec<crate::domain::whyte::streams::StreamRow>> {
    Json(state.streams.list().await)
}

/// A player's socket. The first message names them; everything after is
/// a frame, relayed to whoever is watching.
pub async fn broadcast(ws: WebSocketUpgrade, State(state): State<AppState>) -> axum::response::Response {
    ws.on_upgrade(move |socket| run_broadcast(socket, state))
}

async fn run_broadcast(mut socket: WebSocket, state: AppState) {
    // The name first, or nothing happens. Three letters, the same rule
    // the leaderboard uses.
    let initials = match socket.recv().await {
        Some(Ok(Message::Text(t))) => {
            let who = t.trim().to_uppercase();
            if who.len() != 3 || !who.chars().all(|c| c.is_ascii_uppercase()) {
                return;
            }
            who
        }
        _ => return,
    };

    let (id, live) = state.streams.open(initials).await;
    // The player is told their own id so the page can say it is live.
    let _ = socket.send(Message::Text(format!("{{\"id\":\"{id}\"}}").into())).await;

    while let Some(Ok(msg)) = socket.recv().await {
        let Message::Text(frame) = msg else { continue };
        if frame.len() > crate::domain::whyte::streams::MAX_FRAME {
            continue;
        }
        // The distance is read out for the list; the rest is relayed
        // without being understood, because the renderer at the other
        // end is the only thing that needs to.
        if let Some(d) = frame
            .split("\"d\":")
            .nth(1)
            .and_then(|r| r.split(|c: char| !c.is_ascii_digit()).next())
            .and_then(|n| n.parse::<i64>().ok())
        {
            live.metres.store(d, Ordering::Relaxed);
        }
        // An error here means nobody is watching, which is not a
        // problem worth ending a run over.
        let _ = live.frames.send(frame.to_string());
    }

    state.streams.close(id).await;
}

/// A viewer's socket.
pub async fn watch(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> axum::response::Response {
    ws.on_upgrade(move |socket| run_watch(socket, state, id))
}

async fn run_watch(mut socket: WebSocket, state: AppState, id: Uuid) {
    let Some(live) = state.streams.get(id).await else {
        return;
    };
    let mut rx = live.frames.subscribe();
    live.watching.fetch_add(1, Ordering::Relaxed);

    // An error from recv means too far behind to be watching the same
    // run. Dropped rather than buffered — a stalled tab must not hold
    // memory open for everybody else.
    while let Ok(frame) = rx.recv().await {
        if socket.send(Message::Text(frame.into())).await.is_err() {
            break;
        }
    }

    live.watching.fetch_sub(1, Ordering::Relaxed);
}
