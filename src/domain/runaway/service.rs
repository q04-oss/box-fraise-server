//! Anonymous answers to "why do you want to run away?".
//!
//! The only unauthenticated write on the platform, and everything here
//! exists to bound it. 0023 closed the last open write when posting
//! became members-only; this reopens exactly one, deliberately, because
//! a stranger otherwise has no way to take part in anything.
//!
//! It is not a submission and must never become one. A submission
//! carries a member number, which means somebody turned up. These carry
//! nothing. Anonymous gets you read by an editor; a number gets you
//! published in the magazine, and that difference is the reason to walk
//! to a park at eight in the morning.
//!
//! Nothing appears anywhere until an admin accepts it.

use uuid::Uuid;

use super::{
    repository,
    types::{AnswerReceived, AnswerUpload, PendingAnswer, PublishedAnswer},
};
use crate::{
    audit,
    db::{AdminRlsTransaction, Pool},
    error::{AppError, AppResult},
};

/// Mirrors the `runaway_answers_length` CHECK. Twenty characters is not
/// an answer; two thousand is a column, and columns belong to members.
const MIN_CHARS: usize = 20;
const MAX_CHARS: usize = 2000;

/// Backpressure, not a rate limiter — there is no rate-limiting
/// middleware. Once this many are waiting on an editor the door closes
/// until some are dealt with. Without it one script fills the table.
const MAX_PENDING: i32 = 300;

/// How many answers /runaway shows.
const PAGE_LIMIT: i64 = 40;

pub async fn submit(pool: &Pool, upload: AnswerUpload) -> AppResult<AnswerReceived> {
    let body = upload.body.trim();
    let chars = body.chars().count();
    if chars < MIN_CHARS {
        return Err(AppError::bad_request(format!(
            "an answer needs at least {MIN_CHARS} characters"
        )));
    }
    if chars > MAX_CHARS {
        return Err(AppError::bad_request(format!(
            "an answer has to fit in {MAX_CHARS} characters"
        )));
    }

    let mut conn = pool.acquire().await?;
    let pending = repository::pending_count(&mut conn).await?;
    if pending >= MAX_PENDING {
        return Err(AppError::TooManyRequests(
            "there is a backlog of answers waiting to be read; try again in a few days".into(),
        ));
    }
    let id = repository::insert(&mut conn, body).await?;

    // 'public' rather than 'system': no user, no admin, nobody. The
    // metadata records shape and never the writing — audit_events is
    // append-only, so anything put in it could never come out, and an
    // anonymous answer is exactly the thing somebody would later want
    // gone.
    audit::write(
        pool,
        "public",
        None,
        "runaway.answered",
        Some(&id.to_string()),
        serde_json::json!({ "chars": chars }),
    )
    .await;

    Ok(AnswerReceived {
        status: "pending".into(),
    })
}

pub async fn list_published(pool: &Pool) -> AppResult<Vec<PublishedAnswer>> {
    let mut conn = pool.acquire().await?;
    Ok(repository::list_published(&mut conn, PAGE_LIMIT).await?)
}

pub async fn list_pending(pool: &Pool) -> AppResult<Vec<PendingAnswer>> {
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let rows = repository::list_pending(tx.conn()).await?;
    tx.commit().await?;
    Ok(rows)
}

pub async fn accept(pool: &Pool, admin_id: Uuid, id: Uuid) -> AppResult<()> {
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let done = repository::accept(tx.conn(), admin_id, id).await?;
    tx.commit().await?;
    if !done {
        return Err(AppError::Conflict);
    }
    audit::write(
        pool,
        "admin",
        Some(admin_id),
        "runaway.published",
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
        "runaway.rejected",
        Some(&id.to_string()),
        serde_json::json!({}),
    )
    .await;
    Ok(())
}
