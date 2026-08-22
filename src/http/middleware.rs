// Auth middleware: token → identity marker.
//
// Why a soft middleware: routes are public, user-authed, OR admin-authed.
// Rather than three middleware variants, we run one optional pass that
// annotates the request when a token resolves, and let extractors
// (AuthedUser / AuthedAdmin) enforce. The middleware never rejects —
// unknown / expired / missing tokens just leave no marker.
//
// Two places a token can come from, checked in that order:
//
//   Authorization: Bearer <token>   the iOS app, the admin tool, tests
//   Cookie: bf_session=<token>      a member's browser
//
// The header wins so that a request naming a credential explicitly is
// never quietly answered as somebody else who happens to have a cookie
// in the same browser. See src/http/cookies.rs for why a member's
// credential moved out of localStorage.
//
// The session-table lookup runs against the bare pool, NOT inside a
// transaction. There is no `app.user_id` yet — we're literally about to
// derive it. The user_sessions / admin_sessions SELECT policies are
// USING(true) precisely to make this single bootstrap step possible;
// see the migration's notes. Keep this code path narrow — it should
// only ever read by token_hash, and nothing else.

use axum::{
    body::Body,
    extract::State,
    http::{header::AUTHORIZATION, Request, StatusCode},
    middleware::Next,
    response::Response,
};
use uuid::Uuid;

use crate::{
    app::AppState,
    crypto::sha256_hex,
    http::{
        cookies,
        extractors::{
            AuthedAdmin, AuthedRunner, AuthedUser, RunnerSessionHash, SessionHash, SessionToken,
        },
    },
};

pub async fn resolve_bearer(
    State(state): State<AppState>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    if let Some(token) = token_from(&req) {
        let token_hash = sha256_hex(token.as_bytes());

        // User session first — by far the more common path.
        if let Ok(Some((user_id,))) =
            sqlx::query_as::<_, (Uuid,)>("SELECT user_id FROM user_sessions WHERE token_hash = $1")
                .bind(&token_hash)
                .fetch_optional(&state.pool)
                .await
        {
            req.extensions_mut().insert(AuthedUser(user_id));
            // Signing out deletes one session rather than every session
            // this member has, so the handler needs to know which one
            // it was asked as.
            req.extensions_mut().insert(SessionHash(token_hash));
            // And the cookie exchange needs the token itself, to write
            // into Set-Cookie. Both are dropped with the request; only
            // the hash was ever persisted, and still is.
            req.extensions_mut().insert(SessionToken(token));
        } else if let Ok(Some((admin_id,))) = sqlx::query_as::<_, (Uuid,)>(
            "SELECT admin_id FROM admin_sessions
              WHERE token_hash = $1 AND expires_at > now()",
        )
        .bind(&token_hash)
        .fetch_optional(&state.pool)
        .await
        {
            req.extensions_mut().insert(AuthedAdmin(admin_id));
        }
    }

    // Runners are resolved from their own cookie, separately and
    // additionally. Somebody may hold both credentials in one browser —
    // a member who also signed up to the running layer — and neither
    // should displace the other. A runner is never derived from a
    // member's token and cannot be: different cookie, different table.
    if let Some(token) = cookies::get(req.headers(), cookies::RUNNER) {
        let token_hash = sha256_hex(token.as_bytes());
        if let Ok(Some((runner_id,))) = sqlx::query_as::<_, (Uuid,)>(
            "SELECT runner_id FROM runner_sessions
              WHERE token_hash = $1 AND expires_at > now()",
        )
        .bind(&token_hash)
        .fetch_optional(&state.pool)
        .await
        {
            req.extensions_mut().insert(AuthedRunner(runner_id));
            req.extensions_mut().insert(RunnerSessionHash(token_hash));
        }
    }

    Ok(next.run(req).await)
}

fn token_from(req: &Request<Body>) -> Option<String> {
    bearer_from(req).or_else(|| cookies::get(req.headers(), cookies::SESSION))
}

fn bearer_from(req: &Request<Body>) -> Option<String> {
    let value = req.headers().get(AUTHORIZATION)?.to_str().ok()?;
    let prefix = "Bearer ";
    if value.len() > prefix.len() && value[..prefix.len()].eq_ignore_ascii_case(prefix) {
        Some(value[prefix.len()..].trim().to_owned())
    } else {
        None
    }
}
