use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

use super::types::PersonalItem;

/// Every personal item this user owns, ordered starts_at DESC.
/// RLS ensures no other user's rows can appear here regardless of
/// what filters the caller uses — the owner-only SELECT policy is
/// the safety net.
pub async fn list_by_user(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<Vec<PersonalItem>> {
    let rows = sqlx::query_as::<_, PersonalItem>(
        "SELECT id, title, notes, starts_at, ends_at, is_all_day,
                location, created_at, updated_at
           FROM personal_items
          WHERE user_id = $1
          ORDER BY starts_at DESC",
    )
    .bind(user_id)
    .fetch_all(conn)
    .await?;
    Ok(rows)
}

pub async fn get(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<Option<PersonalItem>> {
    let row = sqlx::query_as::<_, PersonalItem>(
        "SELECT id, title, notes, starts_at, ends_at, is_all_day,
                location, created_at, updated_at
           FROM personal_items
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
    user_id: Uuid,
    title: &str,
    notes: Option<&str>,
    starts_at: DateTime<Utc>,
    ends_at: DateTime<Utc>,
    is_all_day: bool,
    location: Option<&str>,
) -> sqlx::Result<PersonalItem> {
    let row = sqlx::query_as::<_, PersonalItem>(
        "INSERT INTO personal_items
            (user_id, title, notes, starts_at, ends_at, is_all_day, location)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING id, title, notes, starts_at, ends_at, is_all_day,
                   location, created_at, updated_at",
    )
    .bind(user_id)
    .bind(title)
    .bind(notes)
    .bind(starts_at)
    .bind(ends_at)
    .bind(is_all_day)
    .bind(location)
    .fetch_one(conn)
    .await?;
    Ok(row)
}

/// COALESCE-based partial update: any field left as NULL in the args
/// keeps its current value. `updated_at` always bumps.
#[allow(clippy::too_many_arguments)]
pub async fn update(
    conn: &mut PgConnection,
    id: Uuid,
    title: Option<&str>,
    notes: Option<Option<&str>>,
    starts_at: Option<DateTime<Utc>>,
    ends_at: Option<DateTime<Utc>>,
    is_all_day: Option<bool>,
    location: Option<Option<&str>>,
) -> sqlx::Result<Option<PersonalItem>> {
    // `notes` and `location` are tri-state:
    //   None                → leave alone
    //   Some(None)          → set to NULL (clear)
    //   Some(Some("value")) → set to value
    // For the SQL side we flatten to Option<&str> plus a "explicit clear"
    // boolean per nullable field.
    let (notes_val, notes_clear) = match notes {
        None => (None, false),
        Some(None) => (None, true),
        Some(Some(v)) => (Some(v), false),
    };
    let (location_val, location_clear) = match location {
        None => (None, false),
        Some(None) => (None, true),
        Some(Some(v)) => (Some(v), false),
    };

    let row = sqlx::query_as::<_, PersonalItem>(
        "UPDATE personal_items
            SET title       = COALESCE($1, title),
                notes       = CASE WHEN $3 THEN NULL ELSE COALESCE($2, notes) END,
                starts_at   = COALESCE($4, starts_at),
                ends_at     = COALESCE($5, ends_at),
                is_all_day  = COALESCE($6, is_all_day),
                location    = CASE WHEN $8 THEN NULL ELSE COALESCE($7, location) END,
                updated_at  = now()
          WHERE id = $9
          RETURNING id, title, notes, starts_at, ends_at, is_all_day,
                    location, created_at, updated_at",
    )
    .bind(title)
    .bind(notes_val)
    .bind(notes_clear)
    .bind(starts_at)
    .bind(ends_at)
    .bind(is_all_day)
    .bind(location_val)
    .bind(location_clear)
    .bind(id)
    .fetch_optional(conn)
    .await?;
    Ok(row)
}

pub async fn delete(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<bool> {
    let result = sqlx::query("DELETE FROM personal_items WHERE id = $1")
        .bind(id)
        .execute(conn)
        .await?;
    Ok(result.rows_affected() > 0)
}
