// RLS plumbing.
//
// Two transaction wrappers, by intent:
//
//   - RlsTransaction      → sets app.user_id transaction-locally
//   - AdminRlsTransaction → sets app.is_admin = 'true' transaction-locally
//
// Always `is_local = true`. Postgres scoping is the whole point — if the
// GUC outlives the request, the next pool checkout sees stale identity
// and we get cross-user data exposure. This is the historic /me-returns-
// empty + cross-account leak class of bug; the fix is to ALWAYS use
// set_config(name, value, true), never plain SET.

use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

pub type Pool = PgPool;

pub async fn connect(database_url: &str) -> anyhow::Result<Pool> {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(20)
        .min_connections(2)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(database_url)
        .await?;
    Ok(pool)
}

/// Transaction-scoped to a single authenticated user. Hold by `&mut`,
/// commit at the end of the request, otherwise drop = rollback.
pub struct RlsTransaction {
    tx: Transaction<'static, Postgres>,
}

impl RlsTransaction {
    pub async fn begin(pool: &Pool, user_id: Uuid) -> sqlx::Result<Self> {
        let mut tx = pool.begin().await?;
        // set_config(key, value, is_local = true) — the `true` is the
        // load-bearing argument; do not change it.
        sqlx::query("SELECT set_config('app.user_id', $1, true)")
            .bind(user_id.to_string())
            .execute(&mut *tx)
            .await?;
        Ok(Self { tx })
    }

    pub fn conn(&mut self) -> &mut sqlx::PgConnection {
        &mut self.tx
    }

    pub async fn commit(self) -> sqlx::Result<()> {
        self.tx.commit().await
    }

    pub async fn rollback(self) -> sqlx::Result<()> {
        self.tx.rollback().await
    }
}

/// Transaction-scoped to "I am an admin." Every admin policy must read
/// app.is_admin explicitly — that is the fix for the historic silent
/// no-op where the GUC was set but no policy referenced it.
pub struct AdminRlsTransaction {
    tx: Transaction<'static, Postgres>,
}

impl AdminRlsTransaction {
    pub async fn begin(pool: &Pool) -> sqlx::Result<Self> {
        let mut tx = pool.begin().await?;
        sqlx::query("SELECT set_config('app.is_admin', 'true', true)")
            .execute(&mut *tx)
            .await?;
        Ok(Self { tx })
    }

    pub fn conn(&mut self) -> &mut sqlx::PgConnection {
        &mut self.tx
    }

    pub async fn commit(self) -> sqlx::Result<()> {
        self.tx.commit().await
    }

    pub async fn rollback(self) -> sqlx::Result<()> {
        self.tx.rollback().await
    }
}
