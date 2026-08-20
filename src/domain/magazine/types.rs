use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Writing sent in for the magazine by somebody who is not a member.
/// Untrusted, and one of two unauthenticated writes on the platform.
#[derive(Deserialize)]
pub struct MagazineUpload {
    pub body: String,
}

#[derive(Serialize)]
pub struct MagazineReceived {
    pub status: String,
}

/// The editor's queue. There is no public counterpart to this type on
/// purpose — nothing in this table is ever served to the web.
#[derive(Serialize, FromRow)]
pub struct PendingMagazineSubmission {
    pub id: Uuid,
    pub body: String,
    pub submitted_at: DateTime<Utc>,
}
