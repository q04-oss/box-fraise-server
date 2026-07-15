use axum::{
    extract::{Path, State},
    routing::get,
    Json, Router,
};

use crate::{
    app::AppState,
    domain::businesses::{service, types::Business},
    error::AppResult,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/businesses", get(public_list))
        .route("/businesses/{slug}", get(public_get_by_slug))
}

async fn public_list(State(state): State<AppState>) -> AppResult<Json<Vec<Business>>> {
    Ok(Json(service::list_public(&state.pool).await?))
}

async fn public_get_by_slug(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> AppResult<Json<Business>> {
    Ok(Json(service::get_by_slug(&state.pool, &slug).await?))
}
