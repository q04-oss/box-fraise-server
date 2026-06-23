// Auth middleware: Bearer token → identity marker.
//
// Why a soft middleware: routes are public, user-authed, OR admin-authed.
// Rather than three middleware variants, we run one optional pass that
// annotates the request when a token resolves, and let extractors
// (AuthedUser / AuthedAdmin) enforce. The middleware never rejects —
// unknown / expired / missing tokens just leave no marker.
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
    http::extractors::{AuthedAdmin, AuthedUser},
};

pub async fn resolve_bearer(
    State(state): State<AppState>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    if let Some(token) = bearer_from(&req) {
        let token_hash = sha256_hex(token.as_bytes());

        // User session first — by far the more common path.
        if let Ok(Some((user_id,))) =
            sqlx::query_as::<_, (Uuid,)>("SELECT user_id FROM user_sessions WHERE token_hash = $1")
                .bind(&token_hash)
                .fetch_optional(&state.pool)
                .await
        {
            req.extensions_mut().insert(AuthedUser(user_id));
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
    Ok(next.run(req).await)
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
