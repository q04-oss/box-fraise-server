use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// One entry on a member's calendar.
///
/// Shifts and runs are different things with different owners, and the
/// member wants them in one list in time order. `kind` is what tells
/// them apart on the page.
#[derive(Serialize, FromRow)]
pub struct CalendarEntry {
    pub kind: String,
    pub id: Uuid,
    /// The business for a shift, the event name for a run.
    pub what: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: Option<DateTime<Utc>>,
    /// Set when a published shift was taken away. The entry stays on
    /// the calendar rather than vanishing, because a member planned
    /// around it and deserves to see that it went.
    pub cancelled_at: Option<DateTime<Utc>>,
}

/// What an admin publishes on a business's behalf. Businesses have no
/// login yet — see the note on `service::publish_shift`.
#[derive(Deserialize)]
pub struct PublishShiftRequest {
    pub member_no: i32,
    pub business_id: Uuid,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct PublishedShift {
    pub id: Uuid,
    pub member_no: i32,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
}

#[derive(Deserialize)]
pub struct RecordEmploymentRequest {
    pub member_no: i32,
    pub business_id: Uuid,
}

#[derive(Serialize)]
pub struct EmploymentRecorded {
    pub id: Uuid,
    pub member_no: i32,
    pub business_id: Uuid,
}
