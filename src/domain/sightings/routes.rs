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
    domain::sightings::{service, types::*},
    error::{AppError, AppResult},
    http::extractors::AuthedAdmin,
};

/// Ceiling on the multipart body, a little above `MAX_IMAGE_BYTES` to
/// leave room for the part headers and the text fields. This rejects
/// an oversized upload at the transport layer, before the bytes are
/// buffered into memory — the service-layer check is the backstop for
/// anything that squeezes under it.
const MAX_UPLOAD_BODY_BYTES: usize = service::MAX_IMAGE_BYTES + 64 * 1024;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/sightings",
            get(public_list).post(submit.layer(DefaultBodyLimit::max(MAX_UPLOAD_BODY_BYTES))),
        )
        .route("/sightings/{id}/image", get(public_image))
        .route("/admin/sightings/pending", get(admin_list_pending))
        .route("/admin/sightings/{id}/image", get(admin_image))
        .route("/admin/sightings/{id}/approve", post(admin_approve))
        .route("/admin/sightings/{id}/reject", post(admin_reject))
}

// ── Public ──────────────────────────────────────────────────────────

async fn public_list(State(state): State<AppState>) -> AppResult<Json<Vec<Sighting>>> {
    Ok(Json(service::list_public(&state.pool).await?))
}

async fn public_image(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<impl IntoResponse> {
    let img = service::image_public(&state.pool, id).await?;
    // Approved sightings are immutable and addressed by UUID, so they
    // are safe to cache hard. A rejected one is deleted, and a deleted
    // id is never reissued.
    Ok(image_response(img, "public, max-age=31536000, immutable"))
}

/// The one public write in the system.
///
/// Multipart parts, all optional except `image`, `latitude` and
/// `longitude`:
///   image           the photo bytes
///   latitude        where the submitter says they saw it
///   longitude       ditto
///   caption         free text, ≤280 chars
///   submitter_name  free text, ≤60 chars
///
/// The coordinates are stated by the uploader — tapped on a map — not
/// read from their device.
///
/// Returns 202 rather than 201: the sighting exists but is not
/// published, and the status code should not imply the submitter
/// created something visible.
async fn submit(
    State(state): State<AppState>,
    multipart: Multipart,
) -> AppResult<(StatusCode, Json<SubmitSightingResponse>)> {
    let upload = parse_multipart(multipart).await?;
    let res = service::submit(&state.pool, upload).await?;
    Ok((StatusCode::ACCEPTED, Json(res)))
}

/// Pull the upload out of the multipart body.
///
/// The declared content type of the `image` part is intentionally
/// ignored — `service::sniff_content_type` decides the format from
/// the bytes. Unknown parts are skipped rather than rejected so an
/// older client sending extra fields keeps working.
async fn parse_multipart(mut multipart: Multipart) -> AppResult<SightingUpload> {
    let mut image_bytes: Option<Vec<u8>> = None;
    let mut latitude: Option<f64> = None;
    let mut longitude: Option<f64> = None;
    let mut caption: Option<String> = None;
    let mut submitter_name: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::bad_request(format!("malformed upload: {e}")))?
    {
        // `name()` borrows the field, so take an owned copy before the
        // body-consuming calls below.
        let Some(name) = field.name().map(str::to_owned) else {
            continue;
        };
        match name.as_str() {
            "image" => {
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| AppError::bad_request(format!("could not read image: {e}")))?;
                image_bytes = Some(bytes.to_vec());
            }
            "latitude" => latitude = field.text().await.ok().and_then(|t| t.trim().parse().ok()),
            "longitude" => longitude = field.text().await.ok().and_then(|t| t.trim().parse().ok()),
            "caption" => caption = field.text().await.ok(),
            "submitter_name" => submitter_name = field.text().await.ok(),
            _ => continue,
        }
    }

    Ok(SightingUpload {
        image_bytes: image_bytes.ok_or_else(|| AppError::bad_request("image part required"))?,
        latitude: latitude
            .ok_or_else(|| AppError::bad_request("pick where you saw it on the map"))?,
        longitude: longitude
            .ok_or_else(|| AppError::bad_request("pick where you saw it on the map"))?,
        caption,
        submitter_name,
    })
}

// ── Admin ───────────────────────────────────────────────────────────

async fn admin_list_pending(
    _admin: AuthedAdmin,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<PendingSighting>>> {
    Ok(Json(service::list_pending(&state.pool).await?))
}

/// Preview of an unapproved sighting. `no-store` because these bytes
/// may be about to be deleted, and because an unmoderated image
/// should not linger in any intermediary cache.
async fn admin_image(
    _admin: AuthedAdmin,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<impl IntoResponse> {
    let img = service::image_admin(&state.pool, id).await?;
    Ok(image_response(img, "no-store"))
}

async fn admin_approve(
    AuthedAdmin(admin_id): AuthedAdmin,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    service::approve(&state.pool, admin_id, id).await?;
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

// ── Shared response shaping ─────────────────────────────────────────

/// Serve stored image bytes.
///
/// `nosniff` matters more here than on a normal endpoint: these bytes
/// came from the public. The content type is re-derived from magic
/// bytes at submit time and constrained by a CHECK at the DB, so it is
/// always one of three known image types — but telling the browser not
/// to second-guess it removes the last content-sniffing foothold.
/// `Content-Disposition: inline` with no filename keeps any
/// submitter-influenced string out of the header.
fn image_response(img: SightingImage, cache_control: &'static str) -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&img.content_type)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(cache_control),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("inline"),
    );
    headers.insert(
        HeaderName::from_static("referrer-policy"),
        HeaderValue::from_static("no-referrer"),
    );
    (StatusCode::OK, headers, img.image_bytes)
}
