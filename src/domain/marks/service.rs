//! Advertisements the scanner recognises.
//!
//! A mark is a picture in the street and what happens when a camera is
//! pointed at it. Registering one is how a sold advertisement goes live
//! — before 0031 it meant editing a JavaScript array and redeploying.
//!
//! The fingerprint is not computed here. The scanner hashes the
//! reference image in the browser with the same code it runs on the
//! camera frame, so there is exactly one implementation of the
//! comparison and no chance of two drifting apart. See 0031.

use uuid::Uuid;

use super::{
    repository,
    types::{AdminMark, MarkUpload, PublicMark, RegisteredMark},
};
use crate::{
    audit,
    db::{AdminRlsTransaction, Pool},
    error::{AppError, AppResult},
};

/// 2 MiB, mirroring the `marks_size_cap` CHECK. Reference artwork is
/// only ever hashed, never displayed, so it does not need to be large —
/// and every scanner load fetches all of them.
pub const MAX_IMAGE_BYTES: usize = 2 * 1024 * 1024;
const MIN_IMAGE_BYTES: usize = 512;

pub async fn list_published(pool: &Pool) -> AppResult<Vec<PublicMark>> {
    let mut conn = pool.acquire().await?;
    Ok(repository::list_published(&mut conn).await?)
}

pub async fn image(pool: &Pool, id: Uuid) -> AppResult<(Vec<u8>, String)> {
    let mut conn = pool.acquire().await?;
    repository::image(&mut conn, id).await?.ok_or(AppError::NotFound)
}

pub async fn list_all(pool: &Pool) -> AppResult<Vec<AdminMark>> {
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let rows = repository::list_all(tx.conn()).await?;
    tx.commit().await?;
    Ok(rows)
}

pub async fn register(
    pool: &Pool,
    admin_id: Uuid,
    upload: MarkUpload,
) -> AppResult<RegisteredMark> {
    let label = upload.label.trim();
    if label.is_empty() {
        return Err(AppError::bad_request("a mark needs a name"));
    }
    if !matches!(upload.act.as_str(), "go" | "line") {
        return Err(AppError::bad_request("act must be 'go' or 'line'"));
    }
    let target = upload.target.as_deref().map(str::trim).filter(|t| !t.is_empty());
    if upload.act == "go" && target.is_none() {
        return Err(AppError::bad_request("a 'go' mark needs somewhere to go"));
    }

    let bytes = upload
        .image_bytes
        .as_deref()
        .filter(|b| !b.is_empty())
        .ok_or_else(|| AppError::bad_request("a mark needs its artwork"))?;
    if bytes.len() < MIN_IMAGE_BYTES {
        return Err(AppError::bad_request("that image is too small to fingerprint"));
    }
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(AppError::bad_request("artwork exceeds 2 MB"));
    }
    // Sniffed from the bytes, never taken from the part header — the
    // same rule as submissions, and for the same reason.
    let content_type = crate::domain::submissions::service::sniff_content_type(bytes)
        .ok_or_else(|| AppError::bad_request("unsupported image (use JPEG, PNG or WebP)"))?;

    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let id = repository::insert(
        tx.conn(),
        label,
        bytes,
        content_type,
        &upload.act,
        target,
        upload.business_id,
        admin_id,
    )
    .await?;
    tx.commit().await?;

    audit::write(
        pool,
        "admin",
        Some(admin_id),
        "mark.registered",
        Some(&id.to_string()),
        serde_json::json!({ "label": label, "act": upload.act, "business_id": upload.business_id }),
    )
    .await;

    Ok(RegisteredMark {
        id,
        label: label.to_owned(),
    })
}

pub async fn set_published(
    pool: &Pool,
    admin_id: Uuid,
    id: Uuid,
    published: bool,
) -> AppResult<()> {
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let done = repository::set_published(tx.conn(), id, published).await?;
    tx.commit().await?;
    if !done {
        return Err(AppError::NotFound);
    }
    audit::write(
        pool,
        "admin",
        Some(admin_id),
        if published { "mark.published" } else { "mark.withdrawn" },
        Some(&id.to_string()),
        serde_json::json!({}),
    )
    .await;
    Ok(())
}

pub async fn delete(pool: &Pool, admin_id: Uuid, id: Uuid) -> AppResult<()> {
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let done = repository::delete(tx.conn(), id).await?;
    tx.commit().await?;
    if !done {
        return Err(AppError::NotFound);
    }
    audit::write(
        pool,
        "admin",
        Some(admin_id),
        "mark.deleted",
        Some(&id.to_string()),
        serde_json::json!({}),
    )
    .await;
    Ok(())
}
