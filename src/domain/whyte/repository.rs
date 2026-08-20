use sqlx::PgConnection;
use uuid::Uuid;

use super::types::BoardRow;

/// The board. Ties break on who got there first.
pub async fn board(conn: &mut PgConnection, limit: i64) -> sqlx::Result<Vec<BoardRow>> {
    sqlx::query_as::<_, BoardRow>(
        "SELECT id, initials, metres, achieved_at
           FROM whyte_scores
          ORDER BY metres DESC, achieved_at ASC
          LIMIT $1",
    )
    .bind(limit)
    .fetch_all(conn)
    .await
}

/// The id is generated here rather than read back — the public role can
/// read this table, so RETURNING would work, but generating it in Rust
/// keeps the write path identical to every other public insert here.
pub async fn insert(conn: &mut PgConnection, initials: &str, metres: i32) -> sqlx::Result<Uuid> {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO whyte_scores (id, initials, metres) VALUES ($1, $2, $3)")
        .bind(id)
        .bind(initials)
        .bind(metres)
        .execute(conn)
        .await?;
    Ok(id)
}

/// How many did better. Rank is that plus one.
pub async fn rank(conn: &mut PgConnection, metres: i32) -> sqlx::Result<i64> {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*)::bigint FROM whyte_scores WHERE metres > $1")
        .bind(metres)
        .fetch_one(conn)
        .await
}

pub async fn delete(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<bool> {
    let done = sqlx::query("DELETE FROM whyte_scores WHERE id = $1")
        .bind(id)
        .execute(conn)
        .await?;
    Ok(done.rows_affected() == 1)
}
