use axum::{
    extract::{Path, State},
    routing::{get, patch},
    Json, Router,
};
use uuid::Uuid;

use crate::{
    app::AppState,
    domain::schedule::{service, types::*},
    error::AppResult,
    http::extractors::AuthedUser,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/me/personal-items", get(list_handler).post(create_handler))
        .route(
            "/me/personal-items/{id}",
            patch(update_handler).delete(delete_handler),
        )
}

async fn list_handler(
    AuthedUser(user_id): AuthedUser,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<PersonalItem>>> {
    Ok(Json(service::list_personal(&state.pool, user_id).await?))
}

async fn create_handler(
    AuthedUser(user_id): AuthedUser,
    State(state): State<AppState>,
    Json(req): Json<CreatePersonalItemRequest>,
) -> AppResult<Json<PersonalItem>> {
    Ok(Json(
        service::create_personal(&state.pool, user_id, req).await?,
    ))
}

async fn update_handler(
    AuthedUser(user_id): AuthedUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdatePersonalItemRequest>,
) -> AppResult<Json<PersonalItem>> {
    Ok(Json(
        service::update_personal(&state.pool, user_id, id, req).await?,
    ))
}

async fn delete_handler(
    AuthedUser(user_id): AuthedUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<axum::http::StatusCode> {
    service::delete_personal(&state.pool, user_id, id).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
