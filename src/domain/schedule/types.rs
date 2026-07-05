use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct PersonalItem {
    pub id: Uuid,
    pub title: String,
    pub notes: Option<String>,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub is_all_day: bool,
    pub location: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreatePersonalItemRequest {
    pub title: String,
    pub notes: Option<String>,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    #[serde(default)]
    pub is_all_day: bool,
    pub location: Option<String>,
}

/// Partial update — only fields the caller explicitly sets are changed.
/// Use `None` to leave a field alone. Setting `notes` or `location` to
/// `Some(null)` clears it (via `serde(default, deserialize_with)` if we
/// ever need tri-state; for now `Some("")` from the client is treated
/// as "clear" at the service layer).
#[derive(Debug, Deserialize, Default)]
pub struct UpdatePersonalItemRequest {
    pub title: Option<String>,
    pub notes: Option<String>,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
    pub is_all_day: Option<bool>,
    pub location: Option<String>,
}
