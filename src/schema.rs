//! Refusing to start when the code is ahead of the database.
//!
//! Migrations are applied by hand. That is a deliberate consequence of
//! the two-role model — `bf_app` has no CREATE privilege, and handing
//! the runtime owner credentials so it could migrate itself would give
//! every bug in this process the ability to rewrite the schema. The
//! trade is that a deploy can ship code whose migration nobody ran.
//!
//! That has happened. 0028 added `submissions.prompt`; the code that
//! selects it reached production first, and the feed answered
//! `{"error":"internal"}` to every reader until somebody noticed. The
//! database was healthy, the binary was healthy, and they disagreed.
//!
//! So the server checks, at boot, that every migration on disk has been
//! recorded in `schema_migrations`, and exits if any has not. The
//! comparison is against the files shipped in the image rather than a
//! list compiled into the binary, because a list is one more thing to
//! forget to update — and forgetting it would silently switch the check
//! off for exactly the migration that needed it.

use std::path::Path;

use crate::db::Pool;

/// Where the migrations live at runtime. The Dockerfile copies the
/// directory into the image rather than baking it into the binary, so
/// this is a path in both development and production.
pub fn migrations_dir() -> String {
    std::env::var("MIGRATIONS_DIR").unwrap_or_else(|_| "migrations".into())
}

/// Every `NNNN_name.sql` in the directory, by stem, sorted.
fn on_disk(dir: &Path) -> anyhow::Result<Vec<String>> {
    let mut found: Vec<String> = std::fs::read_dir(dir)
        .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("sql") {
                return None;
            }
            path.file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_owned())
        })
        .collect();
    found.sort();
    Ok(found)
}

/// Compare the directory against `schema_migrations` and complain about
/// anything unapplied.
///
/// Returns the names that are missing, oldest first. Empty means the
/// database is level with the code.
pub async fn pending(pool: &Pool) -> anyhow::Result<Vec<String>> {
    let dir = migrations_dir();
    let files = on_disk(Path::new(&dir))?;
    if files.is_empty() {
        anyhow::bail!("no migrations found in {dir} — is MIGRATIONS_DIR right?");
    }

    // A database with no schema_migrations table at all is one that has
    // never had 0030 run. Say that plainly rather than failing on a
    // missing relation.
    let table_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM information_schema.tables
                         WHERE table_schema = 'public'
                           AND table_name = 'schema_migrations')",
    )
    .fetch_one(pool)
    .await?;
    if !table_exists {
        return Ok(files);
    }

    let applied: Vec<String> = sqlx::query_scalar("SELECT version FROM schema_migrations")
        .fetch_all(pool)
        .await?;

    Ok(files
        .into_iter()
        .filter(|f| !applied.iter().any(|a| a == f))
        .collect())
}

/// Boot gate. Logs what is missing and returns an error, so `main` can
/// exit before binding a port and serving requests it cannot answer.
///
/// Deliberately fails on *any* unapplied migration rather than trying
/// to work out which ones matter. A migration that turns out to be
/// harmless costs one psql command; one that is not costs an outage
/// nobody sees.
pub async fn verify(pool: &Pool) -> anyhow::Result<()> {
    let missing = pending(pool).await?;
    if missing.is_empty() {
        tracing::info!("schema is level with the code");
        return Ok(());
    }

    for name in &missing {
        tracing::error!(migration = %name, "migration has not been applied");
    }
    anyhow::bail!(
        "the database is behind this build by {} migration(s): {}. \
         Apply them and start again — the app cannot do it itself, \
         because bf_app has no CREATE privilege and should not.",
        missing.len(),
        missing.join(", ")
    )
}
