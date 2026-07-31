use serde_json::json;
use uuid::Uuid;

use crate::{
    audit,
    db::{AdminRlsTransaction, Pool},
    domain::sightings::{repository, types::*},
    error::{AppError, AppResult},
};

/// Hard byte cap on a stored photo. Enforced in three independent
/// places: `DefaultBodyLimit` on the route (rejects before we
/// buffer), this constant (rejects after decode), and the
/// `sightings_size_cap` CHECK constraint (rejects at the DB). If you
/// change it, change all three — migration 0014 names this constant
/// in its comment for exactly that reason.
pub const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;

/// Anything smaller than this is not a real photo. Guards against
/// empty parts and truncated uploads producing rows we would have to
/// moderate by hand.
const MIN_IMAGE_BYTES: usize = 512;

/// Ceiling on unreviewed submissions across the whole map. There is
/// no rate limiter in front of this service, so without a cap one
/// script could bury the moderation queue and fill the database with
/// image bytes.
///
/// Be clear about what this is not: a determined flooder can still
/// reach the cap and, while the queue is full, keep honest
/// submissions out. It bounds storage, not abuse. Clearing the queue
/// is what restores service.
const MAX_PENDING: i32 = 300;

/// Mirrors the CHECK constraints on the caption/submitter columns.
const MAX_CAPTION_CHARS: usize = 280;
const MAX_SUBMITTER_CHARS: usize = 60;

// ── Public reads ────────────────────────────────────────────────────

/// The map. Plain transaction, so the sightings SELECT policy
/// resolves to approved-only.
pub async fn list_public(pool: &Pool) -> AppResult<Vec<Sighting>> {
    let mut tx = pool.begin().await?;
    let rows = repository::list_approved(&mut tx).await?;
    tx.commit().await?;
    Ok(rows)
}

/// Public image bytes. Runs on a plain transaction on purpose: RLS is
/// what stops an unapproved photo being served, so this endpoint
/// stays safe even if someone guesses a pending sighting's UUID.
pub async fn image_public(pool: &Pool, id: Uuid) -> AppResult<SightingImage> {
    let mut tx = pool.begin().await?;
    let img = repository::get_image(&mut tx, id).await?;
    tx.commit().await?;
    img.ok_or(AppError::NotFound)
}

// ── Public write ────────────────────────────────────────────────────

/// Accept a sighting.
///
/// Everything a submitter sends is untrusted: the bytes, the declared
/// content type, the coordinates, the caption, the name. Cheap checks
/// first, DB work last, so a garbage upload holds a connection for as
/// short a time as possible.
pub async fn submit(pool: &Pool, upload: SightingUpload) -> AppResult<SubmitSightingResponse> {
    if upload.image_bytes.len() < MIN_IMAGE_BYTES {
        return Err(AppError::bad_request("image missing or too small"));
    }
    if upload.image_bytes.len() > MAX_IMAGE_BYTES {
        return Err(AppError::bad_request("image exceeds 8 MB"));
    }
    if !(-90.0..=90.0).contains(&upload.latitude) || !(-180.0..=180.0).contains(&upload.longitude) {
        return Err(AppError::bad_request("invalid location"));
    }

    // Content type comes from the bytes, never from the client's part
    // header. A caller claiming `image/jpeg` while sending HTML would
    // otherwise get that HTML echoed back from the image endpoint
    // under an image content type — the classic stored-XSS-via-upload
    // shape.
    let content_type = sniff_content_type(&upload.image_bytes)
        .ok_or_else(|| AppError::bad_request("unsupported image format (use JPEG, PNG or WebP)"))?;

    let caption = clean_optional_text(upload.caption.as_deref(), MAX_CAPTION_CHARS)?;
    let submitter_name =
        clean_optional_text(upload.submitter_name.as_deref(), MAX_SUBMITTER_CHARS)?;

    let mut tx = pool.begin().await?;

    let pending = repository::pending_count(&mut tx).await?;
    if pending >= MAX_PENDING {
        tx.commit().await?;
        return Err(AppError::TooManyRequests(
            "there are too many sightings awaiting review; try again later".into(),
        ));
    }

    let id = repository::insert_sighting(
        &mut tx,
        &upload.image_bytes,
        content_type,
        upload.latitude,
        upload.longitude,
        caption.as_deref(),
        submitter_name.as_deref(),
    )
    .await?;
    tx.commit().await?;

    // 'public' actor: no user, no admin, no server. Added to the
    // actor_type vocabulary in migration 0012.
    //
    // Note the coordinates are recorded here. They are not personal
    // location data — they are where the submitter says a sticker is,
    // stated deliberately, and they become public on approval anyway.
    audit::write(
        pool,
        "public",
        None,
        "sighting.submit",
        Some(&id.to_string()),
        json!({
            "latitude": upload.latitude,
            "longitude": upload.longitude,
            "content_type": content_type,
            "byte_size": upload.image_bytes.len(),
        }),
    )
    .await;

    Ok(SubmitSightingResponse {
        id,
        status: "pending".into(),
    })
}

// ── Admin ───────────────────────────────────────────────────────────

pub async fn list_pending(pool: &Pool) -> AppResult<Vec<PendingSighting>> {
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let rows = repository::list_pending(tx.conn()).await?;
    tx.commit().await?;
    Ok(rows)
}

/// Admin preview of any sighting, pending included. This is the only
/// read path that can see unapproved bytes, and it is gated by the
/// AuthedAdmin extractor on the route plus `app.is_admin` here.
pub async fn image_admin(pool: &Pool, id: Uuid) -> AppResult<SightingImage> {
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let img = repository::get_image(tx.conn(), id).await?;
    tx.commit().await?;
    img.ok_or(AppError::NotFound)
}

pub async fn approve(pool: &Pool, admin_id: Uuid, id: Uuid) -> AppResult<()> {
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let approved = repository::approve(tx.conn(), id, admin_id).await?;
    tx.commit().await?;

    // None means the row was not pending: either already approved by a
    // concurrent admin, or gone. Conflict rather than NotFound so the
    // queue UI can just refresh.
    if approved.is_none() {
        return Err(AppError::Conflict);
    }

    audit::write(
        pool,
        "admin",
        Some(admin_id),
        "sighting.approve",
        Some(&id.to_string()),
        json!({}),
    )
    .await;
    Ok(())
}

/// Reject = delete. The bytes go, the audit row stays.
pub async fn reject(pool: &Pool, admin_id: Uuid, id: Uuid) -> AppResult<()> {
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let deleted = repository::delete(tx.conn(), id).await?;
    tx.commit().await?;

    if deleted.is_none() {
        return Err(AppError::NotFound);
    }

    audit::write(
        pool,
        "admin",
        Some(admin_id),
        "sighting.reject",
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
/// The allowlist matches the `sightings_content_type_allowed` CHECK
/// constraint. Note what is absent: SVG (it is a script container,
/// not an image, and would be a stored-XSS vector even with
/// `nosniff`) and HEIC (browsers outside Safari cannot render it —
/// the page re-encodes to JPEG client-side before upload, so iPhone
/// photos arrive here as JPEG).
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
/// which counts characters, not bytes — a caption of 280 emoji is
/// fine here and would fail the CHECK constraint if we measured
/// `len()`.
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
