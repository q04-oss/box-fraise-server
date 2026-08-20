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
    /// Whether the same artwork also appears as a billboard in the
    /// game. Off unless somebody says so — see 0036.
    pub in_game: bool,
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
    /// Times its billboard has been run past, all time. The figure a
    /// business is quoted.
    pub impressions: i64,
    pub act: String,
    pub target: Option<String>,
    pub published: bool,
    pub in_game: bool,
    pub business_name: Option<String>,
}

/// What the game reports after a run: a mark and how many times its
/// billboard scrolled past. Batched, so a run is one request rather
/// than one per billboard.
#[derive(Deserialize)]
pub struct ImpressionBatch {
    pub seen: Vec<Impression>,
}

#[derive(Deserialize)]
pub struct Impression {
    pub id: Uuid,
    pub seen: i32,
}

/// A billboard in the game. No act and no target: it is scenery a
/// runner passes, not something to press.
#[derive(Serialize, FromRow)]
pub struct Billboard {
    pub id: Uuid,
    pub label: String,
}

#[derive(Deserialize)]
pub struct MarkImage {
    #[allow(dead_code)]
    pub id: Uuid,
}
