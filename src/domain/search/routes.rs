use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};

use crate::{
    app::AppState,
    domain::search::{service, types::*},
    error::{AppError, AppResult},
};

pub fn router() -> Router<AppState> {
    Router::new().route("/search", get(search_handler))
}

async fn search_handler(
    State(state): State<AppState>,
    Query(params): Query<SearchQuery>,
) -> AppResult<Json<SearchResponse>> {
    let q = params.q.trim();
    if q.is_empty() {
        return Err(AppError::bad_request("query required"));
    }
    // Brave rejects queries > ~400 chars; cap earlier so we fail fast
    // and stay under any per-query cost bump on paid tiers.
    if q.len() > 200 {
        return Err(AppError::bad_request("query too long (max 200 chars)"));
    }
    Ok(Json(service::search(&state.cfg, q).await?))
}
