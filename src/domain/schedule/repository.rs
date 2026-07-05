use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::celestial::calc;

use super::types::PersonalItemRow;

/// Every personal item this user owns, ordered starts_at DESC.
/// RLS ensures no other user's rows can appear here regardless of
/// what filters the caller uses — the owner-only SELECT policy is
/// the safety net.
pub async fn list_by_user(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<Vec<PersonalItemRow>> {
    let rows = sqlx::query_as::<_, PersonalItemRow>(
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

pub async fn get(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<Option<PersonalItemRow>> {
    let row = sqlx::query_as::<_, PersonalItemRow>(
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
) -> sqlx::Result<PersonalItemRow> {
    // Compute the celestial values for starts_at. Persisted so the
    // interpretation stays stable even if the ephemeris improves later.
    let moon_phase = calc::moon_phase(starts_at);
    let moon_long_deg = calc::moon_longitude_deg(starts_at);
    let sun_long_deg = calc::sun_longitude_deg(starts_at);

    let row = sqlx::query_as::<_, PersonalItemRow>(
        "INSERT INTO personal_items
            (user_id, title, notes, starts_at, ends_at, is_all_day, location,
             moon_phase, moon_longitude_deg, sun_longitude_deg)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
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
    .bind(moon_phase)
    .bind(moon_long_deg)
    .bind(sun_long_deg)
    .fetch_one(conn)
    .await?;
    Ok(row)
}

/// COALESCE-based partial update: any field left as NULL in the args
/// keeps its current value. `updated_at` always bumps. If starts_at
/// changes, celestial columns are recomputed to match.
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
) -> sqlx::Result<Option<PersonalItemRow>> {
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

    // Recompute celestial context if starts_at is changing.
    let (moon_phase, moon_long, sun_long) = match starts_at {
        Some(t) => (
            Some(calc::moon_phase(t)),
            Some(calc::moon_longitude_deg(t)),
            Some(calc::sun_longitude_deg(t)),
        ),
        None => (None, None, None),
    };

    let row = sqlx::query_as::<_, PersonalItemRow>(
        "UPDATE personal_items
            SET title              = COALESCE($1, title),
                notes              = CASE WHEN $3 THEN NULL ELSE COALESCE($2, notes) END,
                starts_at          = COALESCE($4, starts_at),
                ends_at            = COALESCE($5, ends_at),
                is_all_day         = COALESCE($6, is_all_day),
                location           = CASE WHEN $8 THEN NULL ELSE COALESCE($7, location) END,
                moon_phase         = COALESCE($10, moon_phase),
                moon_longitude_deg = COALESCE($11, moon_longitude_deg),
                sun_longitude_deg  = COALESCE($12, sun_longitude_deg),
                updated_at         = now()
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
    .bind(moon_phase)
    .bind(moon_long)
    .bind(sun_long)
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
