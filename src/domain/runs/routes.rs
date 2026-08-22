use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use std::sync::atomic::Ordering;
use uuid::Uuid;

use crate::{
    app::AppState,
    domain::runs::{live::RunRow, service, types::*},
    error::{AppError, AppResult},
    http::extractors::AuthedUser,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/runs", get(live).post(start))
        .route("/runs/{id}/at", post(at))
        .route("/runs/{id}", axum::routing::delete(stop))
        .route("/runs/watch/{id}", get(watch))
}

/// Who is out there now. Public, because witnessing is the point — a run
/// nobody can find is not being witnessed.
///
/// What this exposes is a member number, a distance and a watcher count.
/// The positions themselves only go to somebody who opens the socket,
/// and only while the run is happening.
async fn live(State(state): State<AppState>) -> Json<Vec<RunRow>> {
    Json(state.runs.list().await)
}

/// Press the button. Members only, and only current ones.
async fn start(
    AuthedUser(user_id): AuthedUser,
    State(state): State<AppState>,
) -> AppResult<(StatusCode, Json<Started>)> {
    Ok((
        StatusCode::CREATED,
        Json(service::start(&state.pool, &state.runs, user_id).await?),
    ))
}

/// Where they are now.
///
/// A POST rather than a socket held open, because the thing sending
/// these is a modem on a small battery that would rather wake, transmit
/// and sleep. A phone can call it just as easily.
async fn at(
    AuthedUser(user_id): AuthedUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(at): Json<At>,
) -> AppResult<StatusCode> {
    let Some(run) = state.runs.get(id).await else {
        return Err(AppError::NotFound);
    };
    // A run id appears in the public list, so it names a run rather than
    // granting anything. Only the member who started it may say where it
    // is.
    if run.owner != user_id {
        return Err(AppError::Unauthorized);
    }
    if !(-90.0..=90.0).contains(&at.lat) || !(-180.0..=180.0).contains(&at.lon) {
        return Err(AppError::bad_request("that is not a place"));
    }

    run.metres.store(at.m.max(0), Ordering::Relaxed);

    // Five decimal places is about a metre, which is as much as anybody
    // watching a run can use and less than the receiver honestly knows.
    // Rounding here rather than trusting the device keeps the published
    // figure the same whatever is sending it.
    let frame = match at.bpm {
        Some(bpm) if bpm > 20 && bpm < 260 => format!(
            "{{\"lat\":{:.5},\"lon\":{:.5},\"m\":{},\"bpm\":{}}}",
            at.lat, at.lon, at.m, bpm
        ),
        _ => format!(
            "{{\"lat\":{:.5},\"lon\":{:.5},\"m\":{}}}",
            at.lat, at.lon, at.m
        ),
    };
    // An error means nobody is watching, which is not a reason to fail
    // somebody's run.
    let _ = run.positions.send(frame);
    Ok(StatusCode::NO_CONTENT)
}

/// Press the button again. Ending is what makes a run disappear — there
/// is nothing to delete afterwards because nothing was kept.
async fn stop(
    AuthedUser(user_id): AuthedUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let Some(run) = state.runs.get(id).await else {
        return Err(AppError::NotFound);
    };
    if run.owner != user_id {
        return Err(AppError::Unauthorized);
    }
    state.runs.close(id).await;
    Ok(StatusCode::NO_CONTENT)
}

/// A witness's socket. Open to anybody, the same as the game's — there
/// is no account needed to watch somebody run down a public street they
/// chose to broadcast from.
async fn watch(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> axum::response::Response {
    ws.on_upgrade(move |socket| run_watch(socket, state, id))
}

async fn run_watch(mut socket: WebSocket, state: AppState, id: Uuid) {
    let Some(run) = state.runs.get(id).await else {
        return;
    };
    let mut rx = run.positions.subscribe();
    run.watching.fetch_add(1, Ordering::Relaxed);

    // Falling behind means no longer watching the same run. Dropped
    // rather than buffered — a stalled tab must not hold memory open for
    // everybody else.
    while let Ok(frame) = rx.recv().await {
        if socket.send(Message::Text(frame.into())).await.is_err() {
            break;
        }
    }

    run.watching.fetch_sub(1, Ordering::Relaxed);
}
