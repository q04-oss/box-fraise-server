use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use uuid::Uuid;

use crate::{
    app::AppState,
    domain::consultations::{service, types::*},
    error::AppResult,
    http::extractors::{AuthedAdmin, AuthedUser},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/consultations", post(complete_handler))
        .route("/admin/cards/{id}/revoke", post(revoke_handler))
        .route("/admin/cards/{id}/replace", post(replace_handler))
        .route("/cards/{serial}", get(card_lookup_handler))
        .route("/me/verification-status", get(status_handler))
}

async fn complete_handler(
    AuthedAdmin(admin_id): AuthedAdmin,
    State(state): State<AppState>,
    Json(req): Json<CompleteConsultationRequest>,
) -> AppResult<Json<CompleteConsultationResponse>> {
    Ok(Json(
        service::complete_consultation(&state.pool, admin_id, req).await?,
    ))
}

async fn revoke_handler(
    AuthedAdmin(admin_id): AuthedAdmin,
    State(state): State<AppState>,
    Path(card_id): Path<Uuid>,
    Json(req): Json<RevokeCardRequest>,
) -> AppResult<Json<IdentityCard>> {
    Ok(Json(
        service::revoke_card(&state.pool, admin_id, card_id, req).await?,
    ))
}

async fn replace_handler(
    AuthedAdmin(admin_id): AuthedAdmin,
    State(state): State<AppState>,
    Path(card_id): Path<Uuid>,
    Json(req): Json<ReplaceCardRequest>,
) -> AppResult<Json<IdentityCard>> {
    Ok(Json(
        service::replace_card(&state.pool, admin_id, card_id, req).await?,
    ))
}

async fn card_lookup_handler(
    State(state): State<AppState>,
    Path(serial): Path<String>,
) -> AppResult<Json<CardLookupResponse>> {
    Ok(Json(service::lookup_card(&state.pool, &serial).await?))
}

async fn status_handler(
    AuthedUser(user_id): AuthedUser,
    State(state): State<AppState>,
) -> AppResult<Json<MyVerificationStatus>> {
    Ok(Json(
        service::my_verification_status(&state.pool, user_id).await?,
    ))
}
