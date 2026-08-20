use axum::{
    extract::State,
    http::{header::SET_COOKIE, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};

use crate::{
    app::AppState,
    domain::members::{service, types::*},
    error::AppResult,
    http::{
        cookies,
        extractors::{AuthedAdmin, AuthedUser, SecureRequest, SessionHash, SessionToken},
    },
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/members", post(create))
        .route("/admin/members/credential", post(reissue))
        .route("/admin/attendances", post(attend))
        .route("/admin/reach", get(reach))
        .route("/members/me", get(me))
        // One path, two verbs: taking a credential into this browser
        // and giving it up again.
        .route("/members/session", post(adopt).delete(leave))
}

/// Attach the Set-Cookie headers to a response.
///
/// `append` rather than `insert` — there are two cookies and a header
/// map that replaced one with the other would sign somebody half in.
fn with_cookies(body: impl IntoResponse, jar: Vec<HeaderValue>) -> Response {
    let mut res = body.into_response();
    for value in jar {
        res.headers_mut().append(SET_COOKIE, value);
    }
    res
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

/// Replace a member's credential. Admin-only, and in person: this
/// hands somebody else's account to whoever is holding the screen.
async fn reissue(
    AuthedAdmin(admin_id): AuthedAdmin,
    State(state): State<AppState>,
    Json(req): Json<ReissueRequest>,
) -> AppResult<Json<ReissuedCredential>> {
    Ok(Json(service::reissue(&state.pool, admin_id, req).await?))
}

/// The count a business is quoted. Admin-only.
async fn reach(
    AuthedAdmin(_): AuthedAdmin,
    State(state): State<AppState>,
) -> AppResult<Json<Reach>> {
    Ok(Json(service::reach(&state.pool).await?))
}

/// A member's own standing — how long they may keep posting.
async fn me(
    AuthedUser(user_id): AuthedUser,
    State(state): State<AppState>,
) -> AppResult<Json<MembershipStatus>> {
    Ok(Json(service::standing(&state.pool, user_id).await?))
}

/// Take the credential out of the QR code and into a cookie.
///
/// Called once, by /join, with the token it read from the URL fragment.
/// The middleware has already resolved it — reaching this handler at
/// all is the proof that it is a real membership — so all that is left
/// is to write it somewhere durable and say whose it is.
async fn adopt(
    AuthedUser(user_id): AuthedUser,
    SessionToken(token): SessionToken,
    SecureRequest(secure): SecureRequest,
    State(state): State<AppState>,
) -> AppResult<Response> {
    let standing = service::standing(&state.pool, user_id).await?;
    let jar = cookies::sign_in(&token, standing.member_no, secure);
    Ok(with_cookies(Json(standing), jar))
}

/// Sign out of this browser.
///
/// Ends the session server-side as well as clearing the cookies, so a
/// token copied off the device before it was handed back is dead too.
async fn leave(
    AuthedUser(user_id): AuthedUser,
    SessionHash(token_hash): SessionHash,
    SecureRequest(secure): SecureRequest,
    State(state): State<AppState>,
) -> AppResult<Response> {
    service::sign_out(&state.pool, user_id, &token_hash).await?;
    Ok(with_cookies(StatusCode::NO_CONTENT, cookies::sign_out(secure)))
}
