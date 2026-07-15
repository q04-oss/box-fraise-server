use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Raw DB row for an event. Used internally.
#[derive(Debug, sqlx::FromRow)]
pub struct EventRow {
    pub id: Uuid,
    pub name: String,
    pub host_name: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub latitude: f64,
    pub longitude: f64,
    pub address: String,
    pub description: Option<String>,
    pub questions: Vec<String>,
    pub poster_url: Option<String>,
    pub published: bool,
}

#[derive(Debug, Serialize)]
pub struct EventSummary {
    pub id: Uuid,
    pub name: String,
    pub host_name: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub latitude: f64,
    pub longitude: f64,
    pub address: String,
    pub description: Option<String>,
    pub questions: Vec<String>,
    pub poster_url: Option<String>,
    pub published: bool,
}

impl From<EventRow> for EventSummary {
    fn from(row: EventRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            host_name: row.host_name,
            starts_at: row.starts_at,
            ends_at: row.ends_at,
            latitude: row.latitude,
            longitude: row.longitude,
            address: row.address,
            description: row.description,
            questions: row.questions,
            poster_url: row.poster_url,
            published: row.published,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateEventRequest {
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub questions: Vec<String>,
    pub poster_url: Option<String>,
    pub host_name: String,
    pub latitude: f64,
    pub longitude: f64,
    pub address: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub published: bool,
}

#[derive(Debug, Serialize)]
pub struct VerifiedCountResponse {
    pub event_id: Uuid,
    pub verified_count: i64,
}

/// Public archive shape. One entry per event that has any discussion
/// questions attached — past or future, so long as it's published.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct EventQuestions {
    pub event_id: Uuid,
    pub event_name: String,
    pub host_name: String,
    pub starts_at: DateTime<Utc>,
    pub questions: Vec<String>,
}
