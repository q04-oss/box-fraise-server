use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Somewhere you can stand and see a strawberry through your phone's
/// camera.
///
/// Note what is absent: any notion of a visit, a check-in, or who has
/// been here. Whether a visitor is close enough is decided in their
/// browser and never reported back, so the server knows only where
/// the sites are — never where anyone is.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Site {
    pub id: Uuid,
    pub slug: String,
    pub label: String,
    pub blurb: Option<String>,
    pub latitude: f64,
    pub longitude: f64,
    pub sort_order: i32,
    pub published: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateSiteRequest {
    pub slug: String,
    pub label: String,
    pub blurb: Option<String>,
    pub latitude: f64,
    pub longitude: f64,
    #[serde(default)]
    pub sort_order: i32,
    pub published: bool,
}
