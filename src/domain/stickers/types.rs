use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A pin on the map. `photo_count` is the number of *approved* photos
/// — under a non-admin transaction the RLS SELECT policy hides
/// pending rows, so the aggregate counts only what the public can
/// see without needing an explicit status filter.
///
/// `published` is serialized because the admin sticker list reuses
/// this shape to show drafts; on the public list it is always true.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Sticker {
    pub id: Uuid,
    pub slug: String,
    pub label: String,
    pub hint: Option<String>,
    pub latitude: f64,
    pub longitude: f64,
    pub placed_on: Option<NaiveDate>,
    pub sort_order: i32,
    pub published: bool,
    pub photo_count: i64,
    pub created_at: DateTime<Utc>,
}

/// Public gallery entry. Deliberately excludes the bytes — the image
/// is fetched separately from
/// `GET /v1/sticker-photos/{id}/image` so the JSON stays small and
/// the browser can cache each image independently.
///
/// `caption` and `submitter_name` are anonymous free text. Escape
/// them at every render site.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct StickerPhoto {
    pub id: Uuid,
    pub caption: Option<String>,
    pub submitter_name: Option<String>,
    pub submitted_at: DateTime<Utc>,
}

/// Moderation-queue entry. Carries the parent sticker's label/slug so
/// the admin tool can say *which* pin a photo belongs to without an
/// N+1 lookup, plus the size/type the validator settled on.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PendingStickerPhoto {
    pub id: Uuid,
    pub sticker_id: Uuid,
    pub sticker_slug: String,
    pub sticker_label: String,
    pub caption: Option<String>,
    pub submitter_name: Option<String>,
    pub content_type: String,
    pub byte_size: i32,
    pub submitted_at: DateTime<Utc>,
}

/// Raw image for the byte-serving endpoints.
#[derive(Debug, sqlx::FromRow)]
pub struct StickerPhotoImage {
    pub image_bytes: Vec<u8>,
    pub content_type: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateStickerRequest {
    pub slug: String,
    pub label: String,
    pub hint: Option<String>,
    pub latitude: f64,
    pub longitude: f64,
    pub placed_on: Option<NaiveDate>,
    #[serde(default)]
    pub sort_order: i32,
    pub published: bool,
}

/// What a submitter parsed out of the multipart body. `content_type`
/// is NOT taken from the client's part header — the service derives
/// it from the image's magic bytes and overwrites whatever was
/// claimed. See `service::sniff_content_type`.
#[derive(Debug)]
pub struct PhotoUpload {
    pub image_bytes: Vec<u8>,
    pub caption: Option<String>,
    pub submitter_name: Option<String>,
}

/// Submit acknowledgement. Always reports `status: "pending"` — the
/// response deliberately does not pretend the photo is live, so the
/// UI can say "thanks, it'll show up once reviewed."
#[derive(Debug, Serialize)]
pub struct SubmitPhotoResponse {
    pub id: Uuid,
    pub status: String,
}
