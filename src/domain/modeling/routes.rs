use axum::{
    extract::{Path, State},
    routing::{get, patch, post},
    Json, Router,
};
use uuid::Uuid;

use crate::{
    app::AppState,
    domain::modeling::{service, types::*},
    error::AppResult,
    http::extractors::AuthedUser,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/me/hair-profile",
            get(get_hair_profile).patch(patch_hair_profile),
        )
        .route(
            "/students/model-requests",
            get(list_student_requests).post(create_student_request),
        )
        .route(
            "/students/model-requests/{id}/cancel",
            post(cancel_student_request),
        )
        .route("/me/model-invitations", get(list_invitations))
        .route("/me/model-invitations/{id}/accept", post(accept_invitation))
        .route(
            "/me/model-invitations/{id}/decline",
            post(decline_invitation),
        )
        // Fallback alias so the PATCH above is actually reachable
        .route("/me/hair-profile/toggle", patch(patch_hair_profile))
}

async fn get_hair_profile(
    AuthedUser(user_id): AuthedUser,
    State(state): State<AppState>,
) -> AppResult<Json<Option<HairProfile>>> {
    Ok(Json(
        service::get_own_hair_profile(&state.pool, user_id).await?,
    ))
}

async fn patch_hair_profile(
    AuthedUser(user_id): AuthedUser,
    State(state): State<AppState>,
    Json(req): Json<UpdateOwnHairProfileRequest>,
) -> AppResult<Json<HairProfile>> {
    Ok(Json(
        service::update_own_willing_to_model(&state.pool, user_id, req).await?,
    ))
}

async fn list_student_requests(
    AuthedUser(user_id): AuthedUser,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<ModelRequest>>> {
    Ok(Json(
        service::list_own_requests(&state.pool, user_id).await?,
    ))
}

async fn create_student_request(
    AuthedUser(user_id): AuthedUser,
    State(state): State<AppState>,
    Json(req): Json<CreateModelRequestRequest>,
) -> AppResult<Json<CreateModelRequestResponse>> {
    Ok(Json(
        service::create_model_request(&state.pool, user_id, req).await?,
    ))
}

async fn cancel_student_request(
    AuthedUser(user_id): AuthedUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ModelRequest>> {
    Ok(Json(
        service::cancel_own_request(&state.pool, user_id, id).await?,
    ))
}

async fn list_invitations(
    AuthedUser(user_id): AuthedUser,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<InvitationWithContext>>> {
    Ok(Json(
        service::list_own_invitations(&state.pool, user_id).await?,
    ))
}

async fn accept_invitation(
    AuthedUser(user_id): AuthedUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ModelInvitation>> {
    Ok(Json(
        service::respond_to_invitation(&state.pool, user_id, id, true).await?,
    ))
}

async fn decline_invitation(
    AuthedUser(user_id): AuthedUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ModelInvitation>> {
    Ok(Json(
        service::respond_to_invitation(&state.pool, user_id, id, false).await?,
    ))
}
