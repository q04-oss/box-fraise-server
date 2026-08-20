use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// What arrives from the open form. Untrusted, and the only
/// unauthenticated write on the platform.
#[derive(Deserialize)]
pub struct AnswerUpload {
    pub body: String,
}

#[derive(Serialize)]
pub struct AnswerReceived {
    pub status: String,
}

/// A published answer. No author of any kind — that is the whole point
/// of it, and the reason it is not a submission.
#[derive(Serialize, FromRow)]
pub struct PublishedAnswer {
    pub id: Uuid,
    pub body: String,
    pub published_at: DateTime<Utc>,
}

/// The editor's queue.
#[derive(Serialize, FromRow)]
pub struct PendingAnswer {
    pub id: Uuid,
    pub body: String,
    pub submitted_at: DateTime<Utc>,
}
