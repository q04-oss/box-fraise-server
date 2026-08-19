use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

use super::types::CalendarEntry;

/// A member's own calendar: their shifts and the runs, in one list.
///
/// The union is done in SQL rather than in Rust because the two halves
/// are read under different rules and would otherwise need two round
/// trips and a merge. Shifts come from `shifts_own_select`, which
/// scopes them to the caller. Events are public and already filtered to
/// what has been published.
///
/// Cancelled shifts are included on purpose. A member planned around
/// one; it should not simply vanish from the page.
pub async fn mine(
    conn: &mut PgConnection,
    user_id: Uuid,
    from: DateTime<Utc>,
) -> sqlx::Result<Vec<CalendarEntry>> {
    // Each side is limited on its own before the union, never after.
    // A shared LIMIT lets one source crowd out the other: the run club
    // meets twice a week forever, so a member with a shift a fortnight
    // out would watch it fall off the end of their own calendar behind
    // sixty runs. Their shifts are the thing they came for.
    sqlx::query_as::<_, CalendarEntry>(
        "(SELECT 'shift' AS kind, s.id, b.name AS what,
                 s.starts_at, s.ends_at, s.cancelled_at
            FROM shifts s
            JOIN businesses b ON b.id = s.business_id
           WHERE s.user_id = $1 AND s.starts_at >= $2
           ORDER BY s.starts_at ASC
           LIMIT 60)
         UNION ALL
         (SELECT 'run' AS kind, e.id, e.name AS what,
                 e.starts_at, e.ends_at, NULL AS cancelled_at
            FROM events e
           WHERE e.published AND e.starts_at >= $2
           ORDER BY e.starts_at ASC
           LIMIT 20)
         ORDER BY starts_at ASC",
    )
    .bind(user_id)
    .bind(from)
    .fetch_all(conn)
    .await
}

/// Publish a shift. Admin transaction only.
pub async fn insert_shift(
    conn: &mut PgConnection,
    user_id: Uuid,
    business_id: Uuid,
    starts_at: DateTime<Utc>,
    ends_at: DateTime<Utc>,
    admin_id: Uuid,
) -> sqlx::Result<Uuid> {
    sqlx::query_scalar(
        "INSERT INTO shifts
             (user_id, business_id, starts_at, ends_at, published_by_admin_id)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING id",
    )
    .bind(user_id)
    .bind(business_id)
    .bind(starts_at)
    .bind(ends_at)
    .bind(admin_id)
    .fetch_one(conn)
    .await
}

/// Cancel a published shift.
///
/// The times are never touched — a change is a cancellation and a new
/// shift, both on the record, which is what makes "published" mean
/// something. Race-close on `cancelled_at IS NULL` so cancelling twice
/// is a conflict rather than a silent second write.
pub async fn cancel_shift(
    conn: &mut PgConnection,
    id: Uuid,
    admin_id: Uuid,
) -> sqlx::Result<bool> {
    let done = sqlx::query(
        "UPDATE shifts
            SET cancelled_at = now(), cancelled_by_admin_id = $1
          WHERE id = $2 AND cancelled_at IS NULL",
    )
    .bind(admin_id)
    .bind(id)
    .execute(conn)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Record that somebody works somewhere. Returns None when the pairing
/// is already open — the partial unique index refuses the duplicate,
/// and an admin pressing the button twice is not a mistake worth
/// stopping them for.
pub async fn insert_employment(
    conn: &mut PgConnection,
    user_id: Uuid,
    business_id: Uuid,
    admin_id: Uuid,
) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar(
        "INSERT INTO employments (user_id, business_id, recorded_by_admin_id)
         VALUES ($1, $2, $3)
         ON CONFLICT DO NOTHING
         RETURNING id",
    )
    .bind(user_id)
    .bind(business_id)
    .bind(admin_id)
    .fetch_optional(conn)
    .await
}

/// Close an employment. Unlike attendance, this is a status that ends.
pub async fn end_employment(
    conn: &mut PgConnection,
    user_id: Uuid,
    business_id: Uuid,
) -> sqlx::Result<bool> {
    let done = sqlx::query(
        "UPDATE employments
            SET ended_at = now()
          WHERE user_id = $1 AND business_id = $2 AND ended_at IS NULL",
    )
    .bind(user_id)
    .bind(business_id)
    .execute(conn)
    .await?;
    Ok(done.rows_affected() == 1)
}
