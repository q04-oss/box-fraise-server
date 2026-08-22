use sqlx::PgConnection;
use uuid::Uuid;

use super::types::{BoardRow, Run};

pub async fn insert_runner(conn: &mut PgConnection, username: &str) -> sqlx::Result<Uuid> {
    sqlx::query_scalar("INSERT INTO runners (username) VALUES ($1) RETURNING id")
        .bind(username)
        .fetch_one(conn)
        .await
}

pub async fn insert_credentials(
    conn: &mut PgConnection,
    runner_id: Uuid,
    password_hash: &str,
) -> sqlx::Result<()> {
    sqlx::query("INSERT INTO runner_credentials (runner_id, password_hash) VALUES ($1, $2)")
        .bind(runner_id)
        .bind(password_hash)
        .execute(conn)
        .await?;
    Ok(())
}

/// Read for login. Under an admin transaction — `runner_credentials`
/// has no public SELECT policy, the same as `admins`, and this is the
/// only statement in the codebase that touches it by name.
pub async fn credentials_for(
    conn: &mut PgConnection,
    username: &str,
) -> sqlx::Result<Option<(Uuid, String)>> {
    sqlx::query_as(
        "SELECT r.id, c.password_hash
           FROM runners r
           JOIN runner_credentials c ON c.runner_id = r.id
          WHERE r.username = $1",
    )
    .bind(username)
    .fetch_optional(conn)
    .await
}

pub async fn insert_session(
    conn: &mut PgConnection,
    token_hash: &str,
    runner_id: Uuid,
) -> sqlx::Result<()> {
    sqlx::query("INSERT INTO runner_sessions (token_hash, runner_id) VALUES ($1, $2)")
        .bind(token_hash)
        .bind(runner_id)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn delete_session(conn: &mut PgConnection, token_hash: &str) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM runner_sessions WHERE token_hash = $1")
        .bind(token_hash)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn username_of(conn: &mut PgConnection, runner_id: Uuid) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar("SELECT username FROM runners WHERE id = $1")
        .bind(runner_id)
        .fetch_optional(conn)
        .await
}

pub async fn insert_run(
    conn: &mut PgConnection,
    id: Uuid,
    runner_id: Uuid,
    distance_m: i32,
    duration_s: i32,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO logged_runs (id, runner_id, distance_m, duration_s, started_at)
         VALUES ($1, $2, $3, $4, now() - make_interval(secs => $4))",
    )
    .bind(id)
    .bind(runner_id)
    .bind(distance_m)
    .bind(duration_s)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn runs_for(conn: &mut PgConnection, runner_id: Uuid) -> sqlx::Result<Vec<Run>> {
    sqlx::query_as::<_, Run>(
        "SELECT id, distance_m, duration_s, started_at
           FROM logged_runs
          WHERE runner_id = $1
          ORDER BY started_at DESC
          LIMIT 50",
    )
    .bind(runner_id)
    .fetch_all(conn)
    .await
}

/// The score, in SQL so the board and a runner's own page can never
/// disagree about it.
///
/// distance in km × speed in km/h, averaged over the runs logged. Going
/// further raises it, going faster raises it, and logging a hundred easy
/// kilometres does not — which is the point of dividing by the count
/// rather than summing.
const SCORE: &str = "AVG((distance_m / 1000.0) * ((distance_m / 1000.0) / (duration_s / 3600.0)))";

pub async fn board(conn: &mut PgConnection) -> sqlx::Result<Vec<BoardRow>> {
    let sql = format!(
        "SELECT r.username,
                {SCORE}::float8         AS score,
                COUNT(*)::bigint        AS runs,
                SUM(l.distance_m)::bigint AS total_m
           FROM logged_runs l
           JOIN runners r ON r.id = l.runner_id
          GROUP BY r.username
          ORDER BY score DESC
          LIMIT 100"
    );
    sqlx::query_as::<_, BoardRow>(&sql).fetch_all(conn).await
}

pub async fn standing(conn: &mut PgConnection, runner_id: Uuid) -> sqlx::Result<(f64, i64)> {
    let sql = format!(
        "SELECT COALESCE({SCORE}, 0)::float8      AS score,
                COALESCE(SUM(distance_m), 0)::bigint AS total_m
           FROM logged_runs WHERE runner_id = $1"
    );
    sqlx::query_as::<_, (f64, i64)>(&sql)
        .bind(runner_id)
        .fetch_one(conn)
        .await
}
