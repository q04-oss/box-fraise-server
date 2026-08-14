// Background prune.
//
// Keeps three things bounded:
//   - admin_sessions whose expires_at is past
//   - pending users older than PENDING_TTL with no verification
//   - submissions still unreviewed after SUBMISSION_TTL_DAYS
//
// A prune like the third one used to exist for public photo uploads,
// against a table that had already been dropped. Note what that cost:
// prune_once runs as ONE transaction, so a DELETE against a missing
// table aborted the whole tick and the two prunes above silently
// stopped running with it. Every statement here shares that fate —
// keep this function's SQL and the live schema in step.
//
// The submission TTL is long on purpose. Someone sat down and wrote
// that column; deleting it after a month would be rude, and the real
// bound on the table is MAX_PENDING in the submissions service, which
// closes the door at 200. Six months is the point at which nobody is
// coming back for it, and holding user-submitted images indefinitely
// is a liability. Accepted submissions are never pruned — those are
// the ones being made into something.
//
// Pending-user delete cascades to device_keys + challenges + user_sessions
// via the FK ON DELETE CASCADE clauses in the migration.
//
// Deliberately not a job-queue dependency. One tokio::spawn at boot,
// runs every hour. Failures are logged and swallowed; next tick retries.

use std::time::Duration;

use crate::{audit, db::AdminRlsTransaction, db::Pool};

const PRUNE_INTERVAL: Duration = Duration::from_secs(60 * 60); // 1h
const PENDING_TTL_DAYS: i64 = 30;
const SUBMISSION_TTL_DAYS: i64 = 180;

pub fn spawn(pool: Pool) {
    tokio::spawn(async move {
        // Sleep first so the boot path is not slowed by a maintenance
        // tick; the first prune happens after PRUNE_INTERVAL.
        tokio::time::sleep(PRUNE_INTERVAL).await;
        loop {
            if let Err(e) = prune_once(&pool).await {
                tracing::error!(error = ?e, "prune tick failed");
            }
            tokio::time::sleep(PRUNE_INTERVAL).await;
        }
    });
}

async fn prune_once(pool: &Pool) -> anyhow::Result<()> {
    let mut tx = AdminRlsTransaction::begin(pool).await?;

    let expired_sessions = sqlx::query_scalar::<_, i64>(
        "WITH deleted AS (
             DELETE FROM admin_sessions WHERE expires_at < now() RETURNING 1
         ) SELECT COUNT(*)::bigint FROM deleted",
    )
    .fetch_one(tx.conn())
    .await?;

    let stale_pending = sqlx::query_scalar::<_, i64>(
        "WITH deleted AS (
             DELETE FROM users
                   WHERE status = 'pending'
                     AND registered_at < now() - ($1::bigint || ' days')::interval
                   RETURNING 1
         ) SELECT COUNT(*)::bigint FROM deleted",
    )
    .bind(PENDING_TTL_DAYS)
    .fetch_one(tx.conn())
    .await?;

    // Only unreviewed rows are eligible. An accepted submission is
    // material for the magazine and is never pruned on a timer.
    let stale_submissions = sqlx::query_scalar::<_, i64>(
        "WITH deleted AS (
             DELETE FROM submissions
                   WHERE status = 'pending'
                     AND submitted_at < now() - ($1::bigint || ' days')::interval
                   RETURNING 1
         ) SELECT COUNT(*)::bigint FROM deleted",
    )
    .bind(SUBMISSION_TTL_DAYS)
    .fetch_one(tx.conn())
    .await?;

    tx.commit().await?;

    if expired_sessions > 0 || stale_pending > 0 || stale_submissions > 0 {
        tracing::info!(
            expired_admin_sessions = expired_sessions,
            stale_pending_users = stale_pending,
            stale_submissions = stale_submissions,
            "prune tick"
        );
        audit::write(
            pool,
            "system",
            None,
            "maintenance.prune",
            None,
            serde_json::json!({
                "expired_admin_sessions": expired_sessions,
                "stale_pending_users": stale_pending,
                "stale_submissions": stale_submissions,
            }),
        )
        .await;
    }
    Ok(())
}
