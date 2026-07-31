use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

/// One reported sticker: a photo, and where the person says they saw
/// it. This *is* the map pin — there is no separate placed-sticker
/// record behind it.
///
/// Deliberately excludes the bytes. The image is fetched separately
/// from `GET /v1/sightings/{id}/image` so the map JSON stays small
/// and each photo caches independently.
///
/// `caption` and `submitter_name` are anonymous free text. Escape
/// them at every render site.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Sighting {
    pub id: Uuid,
    pub latitude: f64,
    pub longitude: f64,
    pub caption: Option<String>,
    pub submitter_name: Option<String>,
    pub submitted_at: DateTime<Utc>,
}

/// Moderation-queue entry. Carries the size and type the validator
/// settled on, plus the coordinates so the operator can sanity-check
/// where the pin would land before approving it.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PendingSighting {
    pub id: Uuid,
    pub latitude: f64,
    pub longitude: f64,
    pub caption: Option<String>,
    pub submitter_name: Option<String>,
    pub content_type: String,
    pub byte_size: i32,
    pub submitted_at: DateTime<Utc>,
}

/// Raw image for the byte-serving endpoints.
#[derive(Debug, sqlx::FromRow)]
pub struct SightingImage {
    pub image_bytes: Vec<u8>,
    pub content_type: String,
}

/// What a submitter sent, parsed out of the multipart body.
///
/// `content_type` is absent on purpose: the service derives it from
/// the image's magic bytes and ignores whatever the client claimed.
/// The coordinates are stated by the uploader — tapped on a map —
/// not read from their device.
#[derive(Debug)]
pub struct SightingUpload {
    pub image_bytes: Vec<u8>,
    pub latitude: f64,
    pub longitude: f64,
    pub caption: Option<String>,
    pub submitter_name: Option<String>,
}

/// Submit acknowledgement. Always reports `status: "pending"` — the
/// response deliberately does not pretend the sighting is live, so
/// the UI can say "thanks, it'll show up once reviewed."
#[derive(Debug, Serialize)]
pub struct SubmitSightingResponse {
    pub id: Uuid,
    pub status: String,
}
