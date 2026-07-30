use axum::{extract::State, routing::get, Json, Router};

use crate::{
    app::AppState,
    domain::sites::{service, types::*},
    error::AppResult,
    http::extractors::AuthedAdmin,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/sites", get(public_list))
        .route("/admin/sites", get(admin_list).post(admin_create))
}

async fn public_list(State(state): State<AppState>) -> AppResult<Json<Vec<Site>>> {
    Ok(Json(service::list_public(&state.pool).await?))
}

async fn admin_list(
    _admin: AuthedAdmin,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<Site>>> {
    Ok(Json(service::list_admin(&state.pool).await?))
}

async fn admin_create(
    AuthedAdmin(admin_id): AuthedAdmin,
    State(state): State<AppState>,
    Json(req): Json<CreateSiteRequest>,
) -> AppResult<Json<Site>> {
    Ok(Json(service::create(&state.pool, admin_id, req).await?))
}
