use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

/// Public directory row. What GET /v1/businesses returns.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Business {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub website: Option<String>,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
}
