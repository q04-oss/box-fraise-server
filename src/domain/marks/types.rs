use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// A mark as the scanner sees it. No bytes — the image comes from
/// `/v1/marks/{id}/image`, so the list stays small enough to fetch on
/// every page load.
#[derive(Serialize, FromRow)]
pub struct PublicMark {
    pub id: Uuid,
    pub label: String,
    pub act: String,
    pub target: Option<String>,
}

/// What an admin registers. The image arrives as multipart; everything
/// else is a form field.
pub struct MarkUpload {
    pub label: String,
    pub act: String,
    pub target: Option<String>,
    pub business_id: Option<Uuid>,
    pub image_bytes: Option<Vec<u8>>,
}

#[derive(Serialize)]
pub struct RegisteredMark {
    pub id: Uuid,
    pub label: String,
}

#[derive(Serialize, FromRow)]
pub struct AdminMark {
    pub id: Uuid,
    pub label: String,
    pub act: String,
    pub target: Option<String>,
    pub published: bool,
    pub business_name: Option<String>,
}

#[derive(Deserialize)]
pub struct MarkImage {
    #[allow(dead_code)]
    pub id: Uuid,
}
