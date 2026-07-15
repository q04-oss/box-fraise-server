use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

/// Public directory row. What GET /v1/businesses returns.
///
/// `website` is stored for internal reference (the operator vetting
/// a business, future data uses) but is not rendered on the business
/// page: the design decision is that the directory is a signal of
/// participation, not a portal off the platform.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Business {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub website: Option<String>,
    pub location: Option<String>,
    pub slug: Option<String>,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
}
