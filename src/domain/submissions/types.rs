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
    /// Which of the three statements this answers. Untrusted like
    /// everything else here — `Prompt::parse` is what makes it one of
    /// the three, and the `submissions_prompt_known` CHECK is the
    /// backstop.
    pub prompt: String,
}

/// The three things a member can answer.
///
/// Kept as a closed set in Rust as well as in the CHECK, so a typo in a
/// form field is a 400 rather than a row nobody ever reads. The wire
/// values are the stable part; the questions themselves are copy and
/// live in the pages.
pub struct Prompt;

impl Prompt {
    pub const RUN_COUNTRY: &'static str = "run_country";
    pub const RUN_AWAY: &'static str = "run_away";
    pub const BETTER_TASTE: &'static str = "better_taste";

    pub const ALL: [&'static str; 3] = [Self::RUN_COUNTRY, Self::RUN_AWAY, Self::BETTER_TASTE];

    /// None when it is not one of the three.
    pub fn parse(raw: &str) -> Option<&'static str> {
        Self::ALL.into_iter().find(|p| *p == raw.trim())
    }
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
    pub prompt: String,
    pub title: Option<String>,
    pub body: Option<String>,
    pub has_image: bool,
    pub byte_size: Option<i32>,
    pub member_no: i32,
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
    pub prompt: String,
    pub title: Option<String>,
    pub body: Option<String>,
    pub member_no: i32,
    pub has_image: bool,
    pub published_at: DateTime<Utc>,
}
