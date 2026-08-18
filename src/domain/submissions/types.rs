use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

/// What arrives from the public form. Every field is untrusted.
///
/// A submission is a column, a photograph, or both — the service
/// rejects one that is neither, and the `submissions_has_content`
/// CHECK is the backstop.
pub struct SubmissionUpload {
    pub title: Option<String>,
    pub body: Option<String>,
    pub image_bytes: Option<Vec<u8>>,
    pub submitter_name: Option<String>,
}

#[derive(Serialize)]
pub struct SubmitResponse {
    pub id: Uuid,
    pub status: String,
}

/// A row in the editor's queue. Carries the writing itself, so the
/// admin tool can show a column without a second request; the image
/// is fetched separately because it is bytes.
#[derive(Serialize, FromRow)]
pub struct PendingSubmission {
    pub id: Uuid,
    pub title: Option<String>,
    pub body: Option<String>,
    pub has_image: bool,
    pub byte_size: Option<i32>,
    pub submitter_name: Option<String>,
    pub submitted_at: DateTime<Utc>,
}

pub struct SubmissionImage {
    pub bytes: Vec<u8>,
    pub content_type: String,
}

/// A published post, as anyone may read it.
///
/// Columns are named rather than globbed. 0020 makes accepted rows
/// publicly readable, so anything added to this table later is public
/// the moment a post is accepted unless this struct and the query that
/// fills it leave it out. Never widen either to `SELECT *`.
#[derive(Serialize, FromRow)]
pub struct PublishedSubmission {
    pub id: Uuid,
    pub title: Option<String>,
    pub body: Option<String>,
    pub submitter_name: Option<String>,
    pub has_image: bool,
    pub published_at: DateTime<Utc>,
}
