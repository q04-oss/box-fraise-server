use serde_json::json;
use uuid::Uuid;

use crate::{
    audit,
    db::{AdminRlsTransaction, Pool},
    domain::stickers::{repository, types::*},
    error::{AppError, AppResult},
};

/// Hard byte cap on a stored photo. Enforced in three independent
/// places: `DefaultBodyLimit` on the route (rejects before we buffer),
/// this constant (rejects after decode), and the
/// `sticker_photos_size_cap` CHECK constraint (rejects at the DB). If
/// you change it, change all three — migration 0012 names this
/// constant in its comment for exactly that reason.
pub const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;

/// Anything smaller than this is not a real photo. Guards against
/// empty parts and truncated uploads producing rows we'd have to
/// moderate by hand.
const MIN_IMAGE_BYTES: usize = 512;

/// Per-pin ceiling on unreviewed submissions. There is no rate
/// limiter in front of this service, so without a cap one script
/// could bury the moderation queue. Hitting the cap fails the submit
/// with 429 and clears as soon as the operator works through the
/// backlog.
const MAX_PENDING_PER_STICKER: i32 = 50;

/// Mirrors the CHECK constraints on the caption/submitter columns.
const MAX_CAPTION_CHARS: usize = 280;
const MAX_SUBMITTER_CHARS: usize = 60;

// ── Public reads ────────────────────────────────────────────────────

/// The map. Plain transaction, so the stickers SELECT policy resolves
/// to published-only.
pub async fn list_public(pool: &Pool) -> AppResult<Vec<Sticker>> {
    let mut tx = pool.begin().await?;
    let rows = repository::list_published(&mut tx).await?;
    tx.commit().await?;
    Ok(rows)
}

pub async fn get_public(pool: &Pool, slug: &str) -> AppResult<Sticker> {
    let mut tx = pool.begin().await?;
    let row = repository::get_published_by_slug(&mut tx, slug).await?;
    tx.commit().await?;
    row.ok_or(AppError::NotFound)
}

/// Gallery for one pin. 404s on an unknown or unpublished slug rather
/// than returning an empty list, so the client can tell "no such pin"
/// from "nobody has found this one yet."
pub async fn list_photos(pool: &Pool, slug: &str) -> AppResult<Vec<StickerPhoto>> {
    let mut tx = pool.begin().await?;
    let sticker = repository::get_published_by_slug(&mut tx, slug).await?;
    let Some(sticker) = sticker else {
        tx.commit().await?;
        return Err(AppError::NotFound);
    };
    let photos = repository::list_approved_photos(&mut tx, sticker.id).await?;
    tx.commit().await?;
    Ok(photos)
}

/// Public image bytes. Runs on a plain transaction on purpose: RLS is
/// what stops an unapproved photo from being served, so this endpoint
/// stays safe even if someone guesses a pending photo's UUID.
pub async fn photo_image_public(pool: &Pool, photo_id: Uuid) -> AppResult<StickerPhotoImage> {
    let mut tx = pool.begin().await?;
    let img = repository::get_photo_image(&mut tx, photo_id).await?;
    tx.commit().await?;
    img.ok_or(AppError::NotFound)
}

// ── Public write ────────────────────────────────────────────────────

/// Accept a photo submission.
///
/// Everything a submitter sends is untrusted: the bytes, the declared
/// content type, the caption, the name. The order here matters —
/// cheap checks first, DB work last, so a garbage upload costs a
/// connection for as short a time as possible.
pub async fn submit_photo(
    pool: &Pool,
    slug: &str,
    upload: PhotoUpload,
) -> AppResult<SubmitPhotoResponse> {
    if upload.image_bytes.len() < MIN_IMAGE_BYTES {
        return Err(AppError::bad_request("image missing or too small"));
    }
    if upload.image_bytes.len() > MAX_IMAGE_BYTES {
        return Err(AppError::bad_request("image exceeds 8 MB"));
    }

    // Content type comes from the bytes, never from the client's part
    // header. A caller claiming `image/jpeg` while sending HTML would
    // otherwise get that HTML echoed back from the image endpoint with
    // an image content type — the classic stored-XSS-via-upload shape.
    let content_type = sniff_content_type(&upload.image_bytes)
        .ok_or_else(|| AppError::bad_request("unsupported image format (use JPEG, PNG or WebP)"))?;

    let caption = clean_optional_text(upload.caption.as_deref(), MAX_CAPTION_CHARS)?;
    let submitter_name =
        clean_optional_text(upload.submitter_name.as_deref(), MAX_SUBMITTER_CHARS)?;

    let mut tx = pool.begin().await?;

    // Resolve the slug under the public policy: an unpublished pin is
    // invisible here, so submissions against it 404 instead of
    // queueing photos for a sticker that was taken down.
    let sticker = repository::get_published_by_slug(&mut tx, slug).await?;
    let Some(sticker) = sticker else {
        tx.commit().await?;
        return Err(AppError::NotFound);
    };

    let pending = repository::pending_photo_count(&mut tx, sticker.id).await?;
    if pending >= MAX_PENDING_PER_STICKER {
        tx.commit().await?;
        return Err(AppError::TooManyRequests(
            "this sticker has too many photos awaiting review; try again later".into(),
        ));
    }

    let photo_id = repository::insert_photo(
        &mut tx,
        sticker.id,
        &upload.image_bytes,
        content_type,
        caption.as_deref(),
        submitter_name.as_deref(),
    )
    .await?;
    tx.commit().await?;

    // 'public' actor: no user, no admin, no server. Added to the
    // actor_type vocabulary in migration 0012.
    audit::write(
        pool,
        "public",
        None,
        "sticker_photo.submit",
        Some(&photo_id.to_string()),
        json!({
            "sticker_id": sticker.id,
            "sticker_slug": sticker.slug,
            "content_type": content_type,
            "byte_size": upload.image_bytes.len(),
        }),
    )
    .await;

    Ok(SubmitPhotoResponse {
        id: photo_id,
        status: "pending".into(),
    })
}

// ── Admin ───────────────────────────────────────────────────────────

pub async fn list_admin(pool: &Pool) -> AppResult<Vec<Sticker>> {
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let rows = repository::list_all(tx.conn()).await?;
    tx.commit().await?;
    Ok(rows)
}

pub async fn create(pool: &Pool, admin_id: Uuid, req: CreateStickerRequest) -> AppResult<Sticker> {
    let slug = req.slug.trim().to_lowercase();
    if !is_valid_slug(&slug) {
        return Err(AppError::bad_request(
            "slug must be lowercase alphanumeric words separated by single hyphens",
        ));
    }
    let label = req.label.trim();
    if label.is_empty() {
        return Err(AppError::bad_request("label required"));
    }
    if !(-90.0..=90.0).contains(&req.latitude) || !(-180.0..=180.0).contains(&req.longitude) {
        return Err(AppError::bad_request("invalid lat/long"));
    }

    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let sticker = repository::insert_sticker(
        tx.conn(),
        &slug,
        label,
        req.hint.as_deref().map(str::trim).filter(|s| !s.is_empty()),
        req.host.as_deref().map(str::trim).filter(|s| !s.is_empty()),
        req.latitude,
        req.longitude,
        req.placed_on,
        req.sort_order,
        req.published,
    )
    .await?;
    tx.commit().await?;

    audit::write(
        pool,
        "admin",
        Some(admin_id),
        "sticker.create",
        Some(&sticker.id.to_string()),
        json!({ "slug": sticker.slug, "published": sticker.published }),
    )
    .await;

    Ok(sticker)
}

pub async fn list_pending(pool: &Pool) -> AppResult<Vec<PendingStickerPhoto>> {
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let rows = repository::list_pending_photos(tx.conn()).await?;
    tx.commit().await?;
    Ok(rows)
}

/// Admin preview of any photo, pending included. This is the only
/// read path that can see unapproved bytes, and it is gated by the
/// AuthedAdmin extractor on the route plus `app.is_admin` here.
pub async fn photo_image_admin(pool: &Pool, photo_id: Uuid) -> AppResult<StickerPhotoImage> {
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let img = repository::get_photo_image(tx.conn(), photo_id).await?;
    tx.commit().await?;
    img.ok_or(AppError::NotFound)
}

pub async fn approve(pool: &Pool, admin_id: Uuid, photo_id: Uuid) -> AppResult<()> {
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let sticker_id = repository::approve_photo(tx.conn(), photo_id, admin_id).await?;
    tx.commit().await?;

    // None means the row was not pending: either already approved by
    // a concurrent admin, or gone. Conflict rather than NotFound so
    // the queue UI can just refresh.
    let Some(sticker_id) = sticker_id else {
        return Err(AppError::Conflict);
    };

    audit::write(
        pool,
        "admin",
        Some(admin_id),
        "sticker_photo.approve",
        Some(&photo_id.to_string()),
        json!({ "sticker_id": sticker_id }),
    )
    .await;
    Ok(())
}

/// Reject = delete. The bytes go, the audit row stays.
pub async fn reject(pool: &Pool, admin_id: Uuid, photo_id: Uuid) -> AppResult<()> {
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let sticker_id = repository::delete_photo(tx.conn(), photo_id).await?;
    tx.commit().await?;

    let Some(sticker_id) = sticker_id else {
        return Err(AppError::NotFound);
    };

    audit::write(
        pool,
        "admin",
        Some(admin_id),
        "sticker_photo.reject",
        Some(&photo_id.to_string()),
        json!({ "sticker_id": sticker_id }),
    )
    .await;
    Ok(())
}

// ── Validation helpers ──────────────────────────────────────────────

/// Identify the format from its magic bytes. Returns the canonical
/// content type to store, or None for anything we will not serve.
///
/// The allowlist matches the `sticker_photos_content_type_allowed`
/// CHECK constraint. Note what is absent: SVG (it is a script
/// container, not an image, and would be a stored-XSS vector even
/// with `nosniff`) and HEIC (browsers outside Safari cannot render
/// it — the map page re-encodes to JPEG client-side before upload, so
/// iPhone photos arrive here as JPEG).
pub fn sniff_content_type(bytes: &[u8]) -> Option<&'static str> {
    // JPEG: SOI marker followed by the start of any segment.
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("image/jpeg");
    }
    // PNG: the 8-byte signature.
    if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some("image/png");
    }
    // WebP: RIFF container whose form type is WEBP. The 4 bytes
    // between are the chunk length, which we do not care about.
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

/// Mirrors the `stickers_slug_shape` CHECK: lowercase alphanumeric
/// words joined by single hyphens. Validated here too so the operator
/// gets a readable 400 instead of a 500 from a constraint violation.
fn is_valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && !slug.starts_with('-')
        && !slug.ends_with('-')
        && !slug.contains("--")
        && slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}
