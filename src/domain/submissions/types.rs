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
    /// Required. The handle a magic link would claim this post
    /// with, and the only field on a submission that is never public.
    pub submitter_email: String,
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
    pub submitter_email: String,
    pub submitted_at: DateTime<Utc>,
}

pub struct SubmissionImage {
    pub bytes: Vec<u8>,
    pub content_type: String,
}

/// A published post, as anyone may read it.
///
/// Note what is absent: `submitter_email`. RLS is row-level, so the
/// accepted-row policy from 0020 does expose that column to any query
/// the app makes — this struct and the repository query that fills it
/// are the boundary. Never widen either to `SELECT *`.
#[derive(Serialize, FromRow)]
pub struct PublishedSubmission {
    pub id: Uuid,
    pub title: Option<String>,
    pub body: Option<String>,
    pub submitter_name: Option<String>,
    pub has_image: bool,
    pub published_at: DateTime<Utc>,
}
