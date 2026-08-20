use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Multipart, Path, State},
    handler::Handler,
    http::{header, StatusCode},
    response::Response,
    routing::{get, post},
    Json, Router,
};
use uuid::Uuid;

use crate::{
    app::AppState,
    domain::marks::{service, types::*},
    error::{AppError, AppResult},
    http::extractors::AuthedAdmin,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/marks", get(published))
        .route("/marks/billboards", get(billboards))
        .route("/marks/impressions", post(impressions))
        .route("/marks/{id}/image", get(image))
        .route("/admin/marks", get(admin_list))
        .route(
            "/admin/marks",
            post(register.layer(DefaultBodyLimit::max(service::MAX_IMAGE_BYTES + 64 * 1024))),
        )
        .route("/admin/marks/{id}/publish", post(publish))
        .route("/admin/marks/{id}/withdraw", post(withdraw))
        .route("/admin/marks/{id}/delete", post(remove))
}

/// What the scanner loads. Public: it is a page with no session, and
/// these pictures are already stuck to walls in the street.
async fn published(State(state): State<AppState>) -> AppResult<Json<Vec<PublicMark>>> {
    Ok(Json(service::list_published(&state.pool).await?))
}

/// The billboards the game draws. Artwork comes from the same
/// /v1/marks/{id}/image endpoint the scanner uses.
async fn billboards(State(state): State<AppState>) -> AppResult<Json<Vec<Billboard>>> {
    Ok(Json(service::billboards(&state.pool).await?))
}

async fn image(State(state): State<AppState>, Path(id): Path<Uuid>) -> AppResult<Response> {
    let (bytes, content_type) = service::image(&state.pool, id).await?;
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, content_type)
        // Reference artwork does not change once registered.
        .header(header::CACHE_CONTROL, "public, max-age=86400")
        .body(Body::from(bytes))
        .expect("image response"))
}

/// What the game reports after a run. Public: the page has no session,
/// and the row is a mark, a date and a number.
async fn impressions(
    State(state): State<AppState>,
    Json(batch): Json<ImpressionBatch>,
) -> AppResult<StatusCode> {
    service::record_impressions(&state.pool, batch).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn admin_list(
    AuthedAdmin(_): AuthedAdmin,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<AdminMark>>> {
    Ok(Json(service::list_all(&state.pool).await?))
}

async fn register(
    AuthedAdmin(admin_id): AuthedAdmin,
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> AppResult<(StatusCode, Json<RegisteredMark>)> {
    let mut upload = MarkUpload {
        label: String::new(),
        in_game: false,
        act: "go".into(),
        target: None,
        business_id: None,
        image_bytes: None,
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
                    .map_err(|_| AppError::bad_request("could not read the artwork"))?;
                if !bytes.is_empty() {
                    upload.image_bytes = Some(bytes.to_vec());
                }
            }
            "label" => upload.label = field.text().await.unwrap_or_default(),
            "in_game" => {
                upload.in_game = matches!(field.text().await.as_deref(), Ok("1") | Ok("true"));
            }
            "act" => upload.act = field.text().await.unwrap_or_else(|_| "go".into()),
            "target" => upload.target = field.text().await.ok(),
            "business_id" => {
                upload.business_id = field.text().await.ok().and_then(|s| s.parse().ok());
            }
            _ => {}
        }
    }

    Ok((
        StatusCode::CREATED,
        Json(service::register(&state.pool, admin_id, upload).await?),
    ))
}

async fn publish(
    AuthedAdmin(admin_id): AuthedAdmin,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    service::set_published(&state.pool, admin_id, id, true).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn withdraw(
    AuthedAdmin(admin_id): AuthedAdmin,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    service::set_published(&state.pool, admin_id, id, false).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn remove(
    AuthedAdmin(admin_id): AuthedAdmin,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    service::delete(&state.pool, admin_id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
