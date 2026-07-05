use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::celestial::ItemCelestial;

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
    pub published: bool,
}

/// API response: DB row + celestial context at starts_at.
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
    pub published: bool,
    /// Sky context at starts_at — the sign the sun was in, the moon
    /// phase, the season. Every event exists under a specific sky.
    pub celestial: ItemCelestial,
}

impl From<EventRow> for EventSummary {
    fn from(row: EventRow) -> Self {
        let celestial = ItemCelestial::compute(row.starts_at);
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
            published: row.published,
            celestial,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateEventRequest {
    pub name: String,
    pub description: Option<String>,
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
