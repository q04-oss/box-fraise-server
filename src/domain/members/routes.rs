use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};

use crate::{
    app::AppState,
    domain::members::{service, types::*},
    error::AppResult,
    http::extractors::{AuthedAdmin, AuthedUser},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/members", post(create))
        .route("/admin/attendances", post(attend))
        .route("/members/me", get(me))
}

/// Sign somebody up at the run club. Admin-only, because the whole
/// point is that a person did the verifying.
async fn create(
    AuthedAdmin(admin_id): AuthedAdmin,
    State(state): State<AppState>,
    Json(req): Json<CreateMemberRequest>,
) -> AppResult<(StatusCode, Json<CreatedMember>)> {
    Ok((
        StatusCode::CREATED,
        Json(service::create(&state.pool, admin_id, req).await?),
    ))
}

/// Mark somebody present. Admin-only: a member cannot mark themselves,
/// which is the whole point of a membership you have to keep.
async fn attend(
    AuthedAdmin(admin_id): AuthedAdmin,
    State(state): State<AppState>,
    Json(req): Json<RecordAttendanceRequest>,
) -> AppResult<Json<AttendanceRecorded>> {
    Ok(Json(
        service::record_attendance(&state.pool, admin_id, req).await?,
    ))
}

/// A member's own standing — how long they may keep posting.
async fn me(
    AuthedUser(user_id): AuthedUser,
    State(state): State<AppState>,
) -> AppResult<Json<MembershipStatus>> {
    Ok(Json(service::standing(&state.pool, user_id).await?))
}
