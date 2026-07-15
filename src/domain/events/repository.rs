use chrono::Utc;
use sqlx::PgConnection;
use uuid::Uuid;

use super::types::{EventQuestions, EventRow};

/// List upcoming events. RLS does the scoping — under an admin tx the
/// caller sees published + unpublished; under any other tx (anonymous
/// or user-scoped) only published rows are visible.
pub async fn list_upcoming(conn: &mut PgConnection) -> sqlx::Result<Vec<EventRow>> {
    let rows = sqlx::query_as::<_, EventRow>(
        "SELECT id, name, host_name, starts_at, ends_at,
                latitude, longitude, address, description, questions, poster_url, published
           FROM events
          WHERE ends_at >= $1
          ORDER BY starts_at ASC",
    )
    .bind(Utc::now())
    .fetch_all(conn)
    .await?;
    Ok(rows)
}

pub async fn get_by_id(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<Option<EventRow>> {
    let row = sqlx::query_as::<_, EventRow>(
        "SELECT id, name, host_name, starts_at, ends_at,
                latitude, longitude, address, description, questions, poster_url, published
           FROM events
          WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(conn)
    .await?;
    Ok(row)
}

#[allow(clippy::too_many_arguments)]
pub async fn insert(
    conn: &mut PgConnection,
    admin_id: Uuid,
    name: &str,
    description: Option<&str>,
    questions: &[String],
    poster_url: Option<&str>,
    host_name: &str,
    latitude: f64,
    longitude: f64,
    address: &str,
    starts_at: chrono::DateTime<chrono::Utc>,
    ends_at: chrono::DateTime<chrono::Utc>,
    published: bool,
) -> sqlx::Result<EventRow> {
    let row = sqlx::query_as::<_, EventRow>(
        "INSERT INTO events
            (name, description, questions, poster_url, host_name, latitude, longitude, address,
             starts_at, ends_at, published, created_by_admin_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
         RETURNING id, name, host_name, starts_at, ends_at,
                   latitude, longitude, address, description, questions, poster_url, published",
    )
    .bind(name)
    .bind(description)
    .bind(questions)
    .bind(poster_url)
    .bind(host_name)
    .bind(latitude)
    .bind(longitude)
    .bind(address)
    .bind(starts_at)
    .bind(ends_at)
    .bind(published)
    .bind(admin_id)
    .fetch_one(conn)
    .await?;
    Ok(row)
}

/// The full history of discussion questions across all events —
/// published only, in reverse chronological order. Only events
/// with at least one question are included.
pub async fn list_all_questions(conn: &mut PgConnection) -> sqlx::Result<Vec<EventQuestions>> {
    sqlx::query_as::<_, EventQuestions>(
        "SELECT id AS event_id, name AS event_name, host_name, starts_at, questions
           FROM events
          WHERE array_length(questions, 1) IS NOT NULL
          ORDER BY starts_at DESC",
    )
    .fetch_all(conn)
    .await
}

pub async fn verified_count(conn: &mut PgConnection, event_id: Uuid) -> sqlx::Result<i64> {
    let (count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM users WHERE verified_at_event_id = $1")
            .bind(event_id)
            .fetch_one(conn)
            .await?;
    Ok(count)
}
