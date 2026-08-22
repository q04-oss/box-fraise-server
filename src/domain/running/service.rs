//! The running layer: sign up, log a run, be on a board.
//!
//! A runner is not a member. Anybody on the internet may become one with
//! a username and a password, and nobody has verified them. That is the
//! opposite of how a membership works and it is deliberate — see
//! migration 0039. The two populations never touch: separate tables,
//! separate cookie, separate GUC.
//!
//! What is stored about a run is how far, how long and when. There is no
//! route and no coordinates. The browser watches its own position to
//! work out a distance and sends only the total, so the path never
//! leaves the phone and there is nowhere here to put it if it did.
//!
//! The board is unverifiable, the same as the game's leaderboard and for
//! the same reason: the only thing that knows how far somebody ran is
//! the device in their hand. The CHECK constraints in 0039 are a bin for
//! nonsense, not anti-cheat. A number nobody believes is worth nothing,
//! and the honest answer is that this measures who bothered.

use uuid::Uuid;

use super::{
    repository,
    types::{BoardRow, Credentials, LogRun, Me, SignedIn},
};
use crate::{
    audit,
    crypto::{argon2_hash, argon2_verify, sha256_hex},
    db::{AdminRlsTransaction, Pool, RunnerRlsTransaction},
    error::{AppError, AppResult},
};

const MIN_PASSWORD: usize = 8;

fn clean_username(raw: &str) -> AppResult<String> {
    let name = raw.trim().to_lowercase();
    // Mirrors runners_username_shape. Said here so the message is a
    // sentence rather than a constraint violation.
    if name.len() < 3 || name.len() > 20 {
        return Err(AppError::bad_request("a name is three to twenty characters"));
    }
    if !name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
        return Err(AppError::bad_request("letters, numbers and underscores only"));
    }
    Ok(name)
}

/// 32 bytes of system randomness, base64url. The same generator the rest
/// of the codebase mints credentials with.
fn new_token() -> String {
    crate::crypto::new_nonce()
}

pub async fn sign_up(pool: &Pool, req: Credentials) -> AppResult<SignedIn> {
    let username = clean_username(&req.username)?;
    if req.password.chars().count() < MIN_PASSWORD {
        return Err(AppError::bad_request("a password of at least eight characters"));
    }
    let hash = argon2_hash(&req.password).map_err(AppError::Internal)?;
    let token = new_token();

    let mut tx = AdminRlsTransaction::begin(pool).await?;
    // Taken names come back as a unique violation rather than a
    // check-then-insert, which would race with itself.
    let runner_id = match repository::insert_runner(tx.conn(), &username).await {
        Ok(id) => id,
        Err(sqlx::Error::Database(e)) if e.code().as_deref() == Some("23505") => {
            return Err(AppError::bad_request("that name is taken"));
        }
        Err(e) => return Err(e.into()),
    };
    repository::insert_credentials(tx.conn(), runner_id, &hash).await?;
    repository::insert_session(tx.conn(), &sha256_hex(token.as_bytes()), runner_id).await?;
    tx.commit().await?;

    audit::write(
        pool,
        "public",
        None,
        "runner.signed_up",
        Some(&runner_id.to_string()),
        serde_json::json!({}),
    )
    .await;

    Ok(SignedIn { username, token })
}

pub async fn log_in(pool: &Pool, req: Credentials) -> AppResult<SignedIn> {
    let username = clean_username(&req.username)?;

    // Under an admin transaction because `runner_credentials` has no
    // public SELECT — the same shape as admin login, one statement, by
    // name. Nothing user-controlled reaches this except the username.
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let found = repository::credentials_for(tx.conn(), &username).await?;

    let Some((runner_id, hash)) = found else {
        tx.commit().await?;
        // The same message either way. Whether a name exists is not
        // something a stranger gets to learn by guessing.
        return Err(AppError::bad_request("that name and password do not match"));
    };
    if !argon2_verify(&req.password, &hash) {
        tx.commit().await?;
        return Err(AppError::bad_request("that name and password do not match"));
    }

    let token = new_token();
    repository::insert_session(tx.conn(), &sha256_hex(token.as_bytes()), runner_id).await?;
    tx.commit().await?;

    Ok(SignedIn { username, token })
}

pub async fn log_out(pool: &Pool, runner_id: Uuid, token_hash: &str) -> AppResult<()> {
    let mut tx = RunnerRlsTransaction::begin(pool, runner_id).await?;
    repository::delete_session(tx.conn(), token_hash).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn me(pool: &Pool, runner_id: Uuid) -> AppResult<Me> {
    let mut tx = RunnerRlsTransaction::begin(pool, runner_id).await?;
    let username = repository::username_of(tx.conn(), runner_id)
        .await?
        .ok_or(AppError::Unauthorized)?;
    let runs = repository::runs_for(tx.conn(), runner_id).await?;
    let (score, total_m) = repository::standing(tx.conn(), runner_id).await?;
    tx.commit().await?;
    Ok(Me {
        username,
        runs,
        score,
        total_m,
    })
}

/// The most a single logged run may be. Mirrors the CHECKs in 0039 so
/// the refusal is a sentence rather than a constraint violation.
const MAX_M: i32 = 200_000;
const MAX_S: i32 = 86_400;

pub async fn log_run(pool: &Pool, runner_id: Uuid, req: LogRun) -> AppResult<()> {
    if req.distance_m <= 100 || req.distance_m > MAX_M {
        return Err(AppError::bad_request("that is not a distance"));
    }
    if req.duration_s < 60 || req.duration_s > MAX_S {
        return Err(AppError::bad_request("a run is at least a minute"));
    }
    let speed = req.distance_m as f64 / req.duration_s as f64;
    if !(0.5..=8.0).contains(&speed) {
        return Err(AppError::bad_request("nobody runs at that speed"));
    }

    let mut tx = RunnerRlsTransaction::begin(pool, runner_id).await?;
    repository::insert_run(tx.conn(), Uuid::new_v4(), runner_id, req.distance_m, req.duration_s)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// Public. The board is the point of the layer.
pub async fn board(pool: &Pool) -> AppResult<Vec<BoardRow>> {
    let mut conn = pool.acquire().await?;
    Ok(repository::board(&mut conn).await?)
}
