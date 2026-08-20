//! The leaderboard for the game on /os.
//!
//! Outside the platform's identity system on purpose. Everything else
//! here is earned by turning up; this is three letters and a distance,
//! and requiring a membership to be on it would be taking a toy
//! seriously. See 0035.

use uuid::Uuid;

use super::{
    repository,
    types::{BoardRow, ScorePosted, ScoreUpload},
};
use crate::{
    db::{AdminRlsTransaction, Pool},
    error::{AppError, AppResult},
};

/// How many the board shows. Ten, because that is how many an arcade
/// cabinet showed and because a longer one stops being a board.
const BOARD_SIZE: i64 = 10;

/// Mirrors `whyte_scores_sane`. Nothing plausible comes near it — the
/// cap is for garbage rather than for anybody good at the game.
const MAX_METRES: i32 = 100_000;

pub async fn board(pool: &Pool) -> AppResult<Vec<BoardRow>> {
    let mut conn = pool.acquire().await?;
    Ok(repository::board(&mut conn, BOARD_SIZE).await?)
}

pub async fn post(pool: &Pool, upload: ScoreUpload) -> AppResult<ScorePosted> {
    // Uppercased rather than rejected: somebody typing lowercase meant
    // the same thing.
    let initials = upload.initials.trim().to_uppercase();
    if initials.chars().count() != 3 || !initials.chars().all(|c| c.is_ascii_uppercase()) {
        return Err(AppError::bad_request("three letters, A to Z"));
    }
    if upload.metres <= 0 || upload.metres > MAX_METRES {
        return Err(AppError::bad_request("that is not a distance"));
    }

    let mut conn = pool.acquire().await?;
    repository::insert(&mut conn, &initials, upload.metres).await?;
    let better = repository::rank(&mut conn, upload.metres).await?;

    // No audit entry. A record of who was playing a game and when is
    // surveillance of nothing, and audit_events is append-only.
    Ok(ScorePosted {
        rank: if better < BOARD_SIZE {
            Some(better + 1)
        } else {
            None
        },
    })
}

/// An admin takes a row off. The answer to three letters spelling
/// something unpleasant is a person deleting it, not a filter.
pub async fn delete(pool: &Pool, id: Uuid) -> AppResult<()> {
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let done = repository::delete(tx.conn(), id).await?;
    tx.commit().await?;
    if done {
        Ok(())
    } else {
        Err(AppError::NotFound)
    }
}
