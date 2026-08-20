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
