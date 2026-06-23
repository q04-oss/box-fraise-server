use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, sqlx::FromRow)]
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
