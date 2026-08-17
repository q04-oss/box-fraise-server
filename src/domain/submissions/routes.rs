use axum::{
    extract::{DefaultBodyLimit, Multipart, Path, State},
    // `Handler` is in scope so the submit handler can carry its own
    // DefaultBodyLimit — the limit belongs to that one route, not the
    // whole router.
    handler::Handler,
    http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json,
    Router,
};
use uuid::Uuid;

use crate::{
    app::AppState,
    domain::submissions::{service, types::*},
    error::{AppError, AppResult},
    http::extractors::AuthedAdmin,
};

/// Ceiling on the multipart body, a little above `MAX_IMAGE_BYTES` to
/// leave room for the part headers and the text fields. This rejects an
/// oversized upload at the transport layer, before the bytes are
/// buffered into memory — the service-layer check is the backstop for
/// anything that squeezes under it.
const MAX_UPLOAD_BODY_BYTES: usize = service::MAX_IMAGE_BYTES + 128 * 1024;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/submissions",
            post(submit.layer(DefaultBodyLimit::max(MAX_UPLOAD_BODY_BYTES))),
        )
        .route("/admin/submissions/pending", get(admin_list_pending))
        .route("/admin/submissions/{id}/image", get(admin_image))
        .route("/admin/submissions/{id}/accept", post(admin_accept))
        .route("/admin/submissions/{id}/reject", post(admin_reject))
}

// ── Public ──────────────────────────────────────────────────────────

/// The one public write in the system.
///
/// There is no GET counterpart. A submission is never readable by
/// anyone but an admin, in any state — see migration 0018.
async fn submit(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> AppResult<(StatusCode, Json<SubmitResponse>)> {
    let mut upload = SubmissionUpload {
        title: None,
        body: None,
        image_bytes: None,
        submitter_name: None,
        submitter_email: String::new(),
    };

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| AppError::bad_request("malformed upload"))?
    {
        match field.name().unwrap_or_default().to_owned().as_str() {
            "image" => {
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|_| AppError::bad_request("could not read the image"))?;
                if !bytes.is_empty() {
                    upload.image_bytes = Some(bytes.to_vec());
                }
            }
            "title" => upload.title = field.text().await.ok(),
            "body" => upload.body = field.text().await.ok(),
            "submitter_name" => upload.submitter_name = field.text().await.ok(),
            "submitter_email" => upload.submitter_email = field.text().await.unwrap_or_default(),
            // Unknown parts are ignored rather than rejected, so an
            // extra field in a future form does not break this one.
            _ => {}
        }
    }

    Ok((
        StatusCode::ACCEPTED,
        Json(service::submit(&state.pool, upload).await?),
    ))
}

// ── Admin ───────────────────────────────────────────────────────────

async fn admin_list_pending(
    AuthedAdmin(_admin_id): AuthedAdmin,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<PendingSubmission>>> {
    Ok(Json(service::list_pending(&state.pool).await?))
}

async fn admin_image(
    AuthedAdmin(_admin_id): AuthedAdmin,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<impl IntoResponse> {
    let img = service::image_admin(&state.pool, id).await?;
    let mut headers = HeaderMap::new();
    if let Ok(value) = HeaderValue::from_str(&img.content_type) {
        headers.insert(header::CONTENT_TYPE, value);
    }
    // Never cached: a pending submission can be deleted at any moment,
    // and it is not public content.
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store, private"),
    );
    // Belt and braces against a byte sequence that slipped the sniffer.
    headers.insert(
        HeaderName::from_static("x-content-type-options"),
        HeaderValue::from_static("nosniff"),
    );
    Ok((headers, img.bytes))
}

async fn admin_accept(
    AuthedAdmin(admin_id): AuthedAdmin,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    service::accept(&state.pool, admin_id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn admin_reject(
    AuthedAdmin(admin_id): AuthedAdmin,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    service::reject(&state.pool, admin_id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
