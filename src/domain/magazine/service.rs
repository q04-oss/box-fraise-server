//! Writing sent in for the magazine by people who are not members.
//!
//! The first edition is made from what members wrote, and there are no
//! members. This is the way out of that: a stranger can send writing to
//! an editor before there is anybody to be a member of anything.
//!
//! Nothing here is ever served to the web. There is no public read of
//! this table in any state — see 0033 — because it is material for
//! print rather than a second feed. A member's post is published under
//! their number; this is read by one person and either kept or deleted.
//!
//! Text only, and there will be no image path. An anonymous upload on a
//! project associated with nudes is the highest-risk endpoint
//! available, and none of the in-person verification that makes the
//! member path defensible applies to somebody nobody has met.

use uuid::Uuid;

use super::{
    repository,
    types::{MagazineReceived, MagazineUpload, PendingMagazineSubmission},
};
use crate::{
    audit,
    db::{AdminRlsTransaction, Pool},
    error::{AppError, AppResult},
};

/// Mirrors the `magazine_submissions_length` CHECK.
const MIN_CHARS: usize = 40;
const MAX_CHARS: usize = 20_000;

/// Backpressure rather than a rate limiter — there is none. Once this
/// many are waiting the door closes until an editor clears some.
const MAX_PENDING: i32 = 200;

pub async fn submit(pool: &Pool, upload: MagazineUpload) -> AppResult<MagazineReceived> {
    let body = upload.body.trim();
    let chars = body.chars().count();
    if chars < MIN_CHARS {
        return Err(AppError::bad_request(format!(
            "a piece needs at least {MIN_CHARS} characters"
        )));
    }
    if chars > MAX_CHARS {
        return Err(AppError::bad_request(format!(
            "a piece has to fit in {MAX_CHARS} characters"
        )));
    }

    let mut conn = pool.acquire().await?;
    let pending = repository::pending_count(&mut conn).await?;
    if pending >= MAX_PENDING {
        return Err(AppError::TooManyRequests(
            "there is a backlog waiting to be read; try again in a few days".into(),
        ));
    }
    let id = repository::insert(&mut conn, body).await?;

    // 'public': no user, no admin, nobody. Length and never the writing
    // — audit_events is append-only, and anonymous writing is exactly
    // what somebody might later want gone.
    audit::write(
        pool,
        "public",
        None,
        "magazine.submitted",
        Some(&id.to_string()),
        serde_json::json!({ "chars": chars }),
    )
    .await;

    Ok(MagazineReceived {
        status: "pending".into(),
    })
}

pub async fn list_pending(pool: &Pool) -> AppResult<Vec<PendingMagazineSubmission>> {
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let rows = repository::list_pending(tx.conn()).await?;
    tx.commit().await?;
    Ok(rows)
}

pub async fn keep(pool: &Pool, admin_id: Uuid, id: Uuid) -> AppResult<()> {
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let done = repository::keep(tx.conn(), admin_id, id).await?;
    tx.commit().await?;
    if !done {
        return Err(AppError::Conflict);
    }
    audit::write(
        pool,
        "admin",
        Some(admin_id),
        "magazine.kept",
        Some(&id.to_string()),
        serde_json::json!({}),
    )
    .await;
    Ok(())
}

pub async fn reject(pool: &Pool, admin_id: Uuid, id: Uuid) -> AppResult<()> {
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let done = repository::delete(tx.conn(), id).await?;
    tx.commit().await?;
    if !done {
        return Err(AppError::NotFound);
    }
    // The only remaining record that it existed.
    audit::write(
        pool,
        "admin",
        Some(admin_id),
        "magazine.rejected",
        Some(&id.to_string()),
        serde_json::json!({}),
    )
    .await;
    Ok(())
}
