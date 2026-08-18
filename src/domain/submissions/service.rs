//! Submissions: columns and photographs sent in for the magazine.
//!
//! Posting is members-only. 0023 puts that in the INSERT policy
//! rather than in this file: a transaction without app.user_id set to
//! the row's owner cannot write here at all. Read migration 0018,
//! 0020 and 0023 before changing anything.

use serde_json::json;
use uuid::Uuid;

use super::{
    repository,
    types::{
        PendingSubmission, PublishedSubmission, SubmissionImage, SubmissionUpload, SubmitResponse,
    },
};
use crate::{
    audit,
    db::{AdminRlsTransaction, Pool, RlsTransaction},
    error::{AppError, AppResult},
};

/// 8 MiB. Mirrored by the `submissions_size_cap` CHECK and by the
/// DefaultBodyLimit on the submit route.
pub const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;
/// Below this it is not a photograph, it is a stub or a truncated
/// upload.
const MIN_IMAGE_BYTES: usize = 512;

/// Backpressure, not a rate limiter — there is no rate-limiting
/// middleware. Once this many submissions are waiting on the editor,
/// the door closes until some are dealt with. Without it a single
/// script could fill the table with 8 MB rows.
const MAX_PENDING: i32 = 200;

const MAX_TITLE_CHARS: usize = 140;
const MAX_BODY_CHARS: usize = 20_000;
/// A column of two words is a mistake or a probe, not a column.
const MIN_BODY_CHARS: usize = 40;

// ── Member write ────────────────────────────────────────────────────

/// Accept a submission.
///
/// Cheap checks first, DB work last, so a garbage upload holds a
/// connection for as short a time as possible.
pub async fn submit(
    pool: &Pool,
    user_id: Uuid,
    upload: SubmissionUpload,
) -> AppResult<SubmitResponse> {
    let title = clean_optional_text(upload.title.as_deref(), MAX_TITLE_CHARS)?;
    let body = clean_optional_text(upload.body.as_deref(), MAX_BODY_CHARS)?;

    if let Some(text) = body.as_deref() {
        if text.chars().count() < MIN_BODY_CHARS {
            return Err(AppError::bad_request(format!(
                "a column needs at least {MIN_BODY_CHARS} characters"
            )));
        }
    }

    // Content type comes from the bytes, never from the client's part
    // header. A caller claiming `image/jpeg` while sending HTML would
    // otherwise get that HTML echoed back from the image endpoint under
    // an image content type — the classic stored-XSS-via-upload shape.
    let image = match upload.image_bytes.as_deref() {
        Some(bytes) if !bytes.is_empty() => {
            if bytes.len() < MIN_IMAGE_BYTES {
                return Err(AppError::bad_request("image too small"));
            }
            if bytes.len() > MAX_IMAGE_BYTES {
                return Err(AppError::bad_request("image exceeds 8 MB"));
            }
            let content_type = sniff_content_type(bytes).ok_or_else(|| {
                AppError::bad_request("unsupported image format (use JPEG, PNG or WebP)")
            })?;
            Some((bytes, content_type))
        }
        _ => None,
    };

    // A submission has to be something. The CHECK constraint says the
    // same thing; this says it in a sentence the sender can read.
    if body.is_none() && image.is_none() {
        return Err(AppError::bad_request(
            "send a column, a photograph, or both",
        ));
    }
    if body.is_none() && title.is_some() {
        return Err(AppError::bad_request("a title needs a column under it"));
    }

    // Under the member's own context: 0023's INSERT policy checks
    // user_id against app.user_id, so a transaction without it cannot
    // write here at all.
    let mut tx = RlsTransaction::begin(pool, user_id).await?;

    // The byline is the member's number, not a field somebody types.
    // Read here because it needs the member's own context —
    // users_self_or_admin_select would hide the row otherwise. It is
    // then denormalised onto the submission: the feed is read with no
    // user context at all, and under RLS a join to users would match
    // nothing and silently drop every post. See 0023 and 0024.
    let member_no = crate::domain::members::repository::member_no(tx.conn(), user_id)
        .await?
        .ok_or_else(|| AppError::bad_request("this account is not a member"))?;

    let pending = repository::pending_count(tx.conn()).await?;
    if pending >= MAX_PENDING {
        tx.commit().await?;
        return Err(AppError::TooManyRequests(
            "there is a backlog of submissions awaiting review; try again in a few days".into(),
        ));
    }

    let id = repository::insert_submission(
        tx.conn(),
        user_id,
        title.as_deref(),
        body.as_deref(),
        image,
        member_no,
    )
    .await?;
    tx.commit().await?;

    // The member is the actor now. Posting is a membership act.
    //
    // The metadata deliberately records shape, not content — lengths
    // and whether there was a photograph, never the writing itself or
    // the sender's contact details. audit_events is append-only, so
    // anything put here can never be taken out.
    audit::write(
        pool,
        "user",
        Some(user_id),
        "submission.received",
        Some(&id.to_string()),
        json!({
            "has_column": body.is_some(),
            "has_image": image.is_some(),
            "body_chars": body.as_deref().map(|b| b.chars().count()),
            "byte_size": image.map(|(b, _)| b.len()),
            "content_type": image.map(|(_, c)| c),
        }),
    )
    .await;

    Ok(SubmitResponse {
        id,
        status: "pending".into(),
    })
}

// ── Public read ────────────────────────────────────────────────────

/// How much of the feed one request returns. A cap rather than
/// pagination: the magazine is a few dozen posts, and an endpoint that
/// can be asked for everything is an endpoint that will be.
const FEED_LIMIT: i64 = 60;

/// The feed. Published posts only, and never anybody's address.
pub async fn list_published(pool: &Pool) -> AppResult<Vec<PublishedSubmission>> {
    let mut conn = pool.acquire().await?;
    Ok(repository::list_published(&mut conn, FEED_LIMIT).await?)
}

pub async fn published_image(pool: &Pool, id: Uuid) -> AppResult<SubmissionImage> {
    let mut conn = pool.acquire().await?;
    repository::published_image(&mut conn, id)
        .await?
        .ok_or(AppError::NotFound)
}

// ── Admin ───────────────────────────────────────────────────────────

pub async fn list_pending(pool: &Pool) -> AppResult<Vec<PendingSubmission>> {
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let rows = repository::list_pending(tx.conn()).await?;
    tx.commit().await?;
    Ok(rows)
}

pub async fn image_admin(pool: &Pool, id: Uuid) -> AppResult<SubmissionImage> {
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let img = repository::image(tx.conn(), id).await?;
    tx.commit().await?;
    img.ok_or(AppError::NotFound)
}

pub async fn accept(pool: &Pool, admin_id: Uuid, id: Uuid) -> AppResult<()> {
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let done = repository::accept(tx.conn(), admin_id, id).await?;
    tx.commit().await?;
    if !done {
        // Either it is gone or another admin got there first.
        return Err(AppError::Conflict);
    }
    audit::write(
        pool,
        "admin",
        Some(admin_id),
        "submission.accepted",
        Some(&id.to_string()),
        json!({}),
    )
    .await;
    Ok(())
}

/// Rejection deletes the row and its bytes. This audit entry is the
/// only remaining record that the submission ever existed, which is
/// why it is written even though the row is gone.
pub async fn reject(pool: &Pool, admin_id: Uuid, id: Uuid) -> AppResult<()> {
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let done = repository::delete(tx.conn(), id).await?;
    tx.commit().await?;
    if !done {
        return Err(AppError::Conflict);
    }
    audit::write(
        pool,
        "admin",
        Some(admin_id),
        "submission.rejected",
        Some(&id.to_string()),
        json!({}),
    )
    .await;
    Ok(())
}

// ── Validation helpers ──────────────────────────────────────────────

/// Identify the format from its magic bytes. Returns the canonical
/// content type to store, or None for anything we will not serve.
///
/// The allowlist matches the `submissions_content_type_allowed` CHECK.
/// Note what is absent: SVG (a script container, not an image, and a
/// stored-XSS vector even with `nosniff`) and HEIC (only Safari renders
/// it — the page re-encodes to JPEG before upload, so iPhone photos
/// arrive here as JPEG).
pub fn sniff_content_type(bytes: &[u8]) -> Option<&'static str> {
    // JPEG: SOI marker followed by the start of any segment.
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("image/jpeg");
    }
    // PNG: the 8-byte signature.
    if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some("image/png");
    }
    // WebP: RIFF container whose form type is WEBP.
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    None
}

/// Trim, reject over-length, and collapse empty to None so the DB
/// stores NULL rather than "".
///
/// Length is measured in `chars()` to match Postgres `char_length`,
/// which counts characters rather than bytes.
fn clean_optional_text(value: Option<&str>, max_chars: usize) -> AppResult<Option<String>> {
    let Some(raw) = value else { return Ok(None) };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.chars().count() > max_chars {
        return Err(AppError::bad_request(format!(
            "text exceeds {max_chars} characters"
        )));
    }
    Ok(Some(trimmed.to_owned()))
}
