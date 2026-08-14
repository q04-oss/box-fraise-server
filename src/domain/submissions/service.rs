//! Submissions: columns and photographs sent in for the magazine.
//!
//! This owns the only unauthenticated write in the system. Everything
//! here exists to bound it — see migration 0018 and CLAUDE.md before
//! changing anything.

use serde_json::json;
use uuid::Uuid;

use super::{
    repository,
    types::{PendingSubmission, SubmissionImage, SubmissionUpload, SubmitResponse},
};
use crate::{
    audit,
    db::{AdminRlsTransaction, Pool},
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
const MAX_NAME_CHARS: usize = 80;
const MAX_CONTACT_CHARS: usize = 200;
/// A column of two words is a mistake or a probe, not a column.
const MIN_BODY_CHARS: usize = 40;

// ── Public write ────────────────────────────────────────────────────

/// Accept a submission.
///
/// Cheap checks first, DB work last, so a garbage upload holds a
/// connection for as short a time as possible.
pub async fn submit(pool: &Pool, upload: SubmissionUpload) -> AppResult<SubmitResponse> {
    let title = clean_optional_text(upload.title.as_deref(), MAX_TITLE_CHARS)?;
    let body = clean_optional_text(upload.body.as_deref(), MAX_BODY_CHARS)?;
    let submitter_name = clean_optional_text(upload.submitter_name.as_deref(), MAX_NAME_CHARS)?;
    let submitter_contact =
        clean_optional_text(upload.submitter_contact.as_deref(), MAX_CONTACT_CHARS)?;

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

    let mut tx = pool.begin().await?;

    let pending = repository::pending_count(&mut tx).await?;
    if pending >= MAX_PENDING {
        tx.commit().await?;
        return Err(AppError::TooManyRequests(
            "there is a backlog of submissions awaiting review; try again in a few days".into(),
        ));
    }

    let id = repository::insert_submission(
        &mut tx,
        title.as_deref(),
        body.as_deref(),
        image,
        submitter_name.as_deref(),
        submitter_contact.as_deref(),
    )
    .await?;
    tx.commit().await?;

    // 'public' actor: no user, no admin, no server.
    //
    // The metadata deliberately records shape, not content — lengths
    // and whether there was a photograph, never the writing itself or
    // the sender's contact details. audit_events is append-only, so
    // anything put here can never be taken out.
    audit::write(
        pool,
        "public",
        None,
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
