use axum::{
    extract::State,
    http::{header::SET_COOKIE, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};

use crate::{
    app::AppState,
    domain::running::{service, types::*},
    error::AppResult,
    http::{
        cookies,
        extractors::{AuthedRunner, RunnerSessionHash, SecureRequest},
    },
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/running/signup", post(signup))
        .route("/running/login", post(login))
        .route("/running/session", axum::routing::delete(logout))
        .route("/running/me", get(me))
        .route("/running/runs", post(log_run))
        .route("/running/board", get(board))
}

fn with_cookies(body: impl IntoResponse, jar: Vec<HeaderValue>) -> Response {
    let mut res = body.into_response();
    for value in jar {
        res.headers_mut().append(SET_COOKIE, value);
    }
    res
}

/// Open to anybody. This is the one place on the platform where an
/// account is created without a person in a park — see migration 0039
/// for why that is a separate population and not a relaxed membership.
async fn signup(
    SecureRequest(secure): SecureRequest,
    State(state): State<AppState>,
    Json(req): Json<Credentials>,
) -> AppResult<Response> {
    let signed = service::sign_up(&state.pool, req).await?;
    let jar = cookies::runner_in(&signed.token, &signed.username, secure);
    Ok(with_cookies(
        (StatusCode::CREATED, Json(serde_json::json!({ "username": signed.username }))),
        jar,
    ))
}

async fn login(
    SecureRequest(secure): SecureRequest,
    State(state): State<AppState>,
    Json(req): Json<Credentials>,
) -> AppResult<Response> {
    let signed = service::log_in(&state.pool, req).await?;
    let jar = cookies::runner_in(&signed.token, &signed.username, secure);
    Ok(with_cookies(
        Json(serde_json::json!({ "username": signed.username })),
        jar,
    ))
}

async fn logout(
    AuthedRunner(runner_id): AuthedRunner,
    RunnerSessionHash(token_hash): RunnerSessionHash,
    SecureRequest(secure): SecureRequest,
    State(state): State<AppState>,
) -> AppResult<Response> {
    service::log_out(&state.pool, runner_id, &token_hash).await?;
    Ok(with_cookies(StatusCode::NO_CONTENT, cookies::runner_out(secure)))
}

async fn me(
    AuthedRunner(runner_id): AuthedRunner,
    State(state): State<AppState>,
) -> AppResult<Json<Me>> {
    Ok(Json(service::me(&state.pool, runner_id).await?))
}

/// A finished run: how far and how long. Never a route — the browser
/// works the distance out from its own position fixes and sends the
/// total, so the path never leaves the phone.
async fn log_run(
    AuthedRunner(runner_id): AuthedRunner,
    State(state): State<AppState>,
    Json(req): Json<LogRun>,
) -> AppResult<StatusCode> {
    service::log_run(&state.pool, runner_id, req).await?;
    Ok(StatusCode::CREATED)
}

/// Public. Anybody can read the board without signing up for anything.
async fn board(State(state): State<AppState>) -> AppResult<Json<Vec<BoardRow>>> {
    Ok(Json(service::board(&state.pool).await?))
}
