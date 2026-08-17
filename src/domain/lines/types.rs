use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// One completion of "for better taste:", as handed to whoever
/// scanned a strawberry. Deliberately thin — the scanner shows the
/// line and who said it, and nothing else.
#[derive(Serialize, FromRow)]
pub struct TasteLine {
    pub id: Uuid,
    pub body: String,
    pub attribution: String,
}

/// The editor's view, which also carries the parts a reader never sees.
#[derive(Serialize, FromRow)]
pub struct AdminTasteLine {
    pub id: Uuid,
    pub body: String,
    pub attribution: String,
    pub source: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Deserialize)]
pub struct CreateLineRequest {
    pub body: String,
    pub attribution: String,
    /// 'editor' | 'business' | 'member'. Defaults to 'editor', which
    /// is the seeded pool.
    pub source: Option<String>,
    /// Publish immediately, or leave it as a draft to sit on.
    #[serde(default)]
    pub publish: bool,
}
