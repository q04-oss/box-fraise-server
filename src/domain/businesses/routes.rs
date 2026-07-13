use axum::{extract::State, routing::get, Json, Router};

use crate::{
    app::AppState,
    domain::businesses::{service, types::Business},
    error::AppResult,
};

pub fn router() -> Router<AppState> {
    Router::new().route("/businesses", get(public_list))
}

async fn public_list(State(state): State<AppState>) -> AppResult<Json<Vec<Business>>> {
    Ok(Json(service::list_public(&state.pool).await?))
}
