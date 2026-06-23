// Serve the admin tool at GET /admin. The HTML is embedded so the
// binary is self-sufficient — no need to ship a separate static dir
// to wherever the server runs.
//
// Camera access in the page requires HTTPS or localhost. In prod, put
// this behind a TLS terminator (nginx / Caddy) before the operator
// can use the QR scanner.

use axum::{
    http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode},
    response::IntoResponse,
    routing::get,
    Router,
};

use crate::app::AppState;

const ADMIN_HTML: &str = include_str!("../../admin/index.html");

pub fn router() -> Router<AppState> {
    Router::new().route("/admin", get(serve_admin))
}

async fn serve_admin() -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    // Defence-in-depth on the admin surface.
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        HeaderName::from_static("referrer-policy"),
        HeaderValue::from_static("no-referrer"),
    );
    (StatusCode::OK, headers, ADMIN_HTML)
}
