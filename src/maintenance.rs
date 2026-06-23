// Background prune.
//
// Keeps two things bounded:
//   - admin_sessions whose expires_at is past
//   - pending users older than PENDING_TTL with no verification
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

    tx.commit().await?;

    if expired_sessions > 0 || stale_pending > 0 {
        tracing::info!(
            expired_admin_sessions = expired_sessions,
            stale_pending_users = stale_pending,
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
            }),
        )
        .await;
    }
    Ok(())
}
