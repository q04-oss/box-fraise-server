use sqlx::PgConnection;
use uuid::Uuid;

use super::types::{PendingSighting, Sighting, SightingImage};

/// Public map: approved sightings, newest first. The `status` filter
/// is explicit rather than relying on RLS alone, so the query means
/// the same thing under an admin transaction as under a public one.
pub async fn list_approved(conn: &mut PgConnection) -> sqlx::Result<Vec<Sighting>> {
    sqlx::query_as::<_, Sighting>(
        "SELECT id, latitude, longitude, caption, submitter_name, submitted_at
           FROM sightings
          WHERE status = 'approved'
          ORDER BY submitted_at DESC",
    )
    .fetch_all(conn)
    .await
}

/// The public write. `status` is not a parameter — it is hardcoded
/// 'pending' here so no request body can influence it. The RLS INSERT
/// policy independently rejects anything else; this is the first of
/// the two barriers.
///
/// The id is generated in Rust instead of being read back with
/// `RETURNING`. Postgres applies the table's SELECT policies to the
/// row an `INSERT ... RETURNING` produces, and a pending sighting is
/// deliberately invisible to `sightings_public_select` — so RETURNING
/// on this path fails with a 42501 even though the insert itself is
/// allowed. Supplying the id means the public write needs no read
/// privilege at all.
#[allow(clippy::too_many_arguments)]
pub async fn insert_sighting(
    conn: &mut PgConnection,
    image_bytes: &[u8],
    content_type: &str,
    latitude: f64,
    longitude: f64,
    caption: Option<&str>,
    submitter_name: Option<&str>,
) -> sqlx::Result<Uuid> {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO sightings
             (id, image_bytes, content_type, byte_size,
              latitude, longitude, caption, submitter_name, status)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'pending')",
    )
    .bind(id)
    .bind(image_bytes)
    .bind(content_type)
    .bind(image_bytes.len() as i32)
    .bind(latitude)
    .bind(longitude)
    .bind(caption)
    .bind(submitter_name)
    .execute(conn)
    .await?;
    Ok(id)
}

/// Global pending count, via the SECURITY DEFINER function added in
/// migration 0014. A public transaction cannot see pending rows to
/// count them itself.
pub async fn pending_count(conn: &mut PgConnection) -> sqlx::Result<i32> {
    sqlx::query_scalar::<_, i32>("SELECT bf_pending_sighting_count()")
        .fetch_one(conn)
        .await
}

/// Image bytes. Visibility is entirely RLS-driven: on a plain
/// transaction only approved sightings resolve, so the public
/// endpoint cannot leak an unmoderated image. The admin endpoint runs
/// the same query under `app.is_admin` to preview the queue.
pub async fn get_image(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<Option<SightingImage>> {
    sqlx::query_as::<_, SightingImage>(
        "SELECT image_bytes, content_type FROM sightings WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(conn)
    .await
}

/// Moderation queue, oldest first so submissions are reviewed in
/// arrival order.
pub async fn list_pending(conn: &mut PgConnection) -> sqlx::Result<Vec<PendingSighting>> {
    sqlx::query_as::<_, PendingSighting>(
        "SELECT id, latitude, longitude, caption, submitter_name,
                content_type, byte_size, submitted_at
           FROM sightings
          WHERE status = 'pending'
          ORDER BY submitted_at ASC",
    )
    .fetch_all(conn)
    .await
}

/// Approve. `AND status = 'pending'` is the race-close, the same
/// idiom as the verify flip: two admins approving concurrently
/// produce exactly one row, and the loser gets None → 409 rather than
/// a second audit entry.
pub async fn approve(
    conn: &mut PgConnection,
    id: Uuid,
    admin_id: Uuid,
) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar::<_, Uuid>(
        "UPDATE sightings
            SET status = 'approved',
                reviewed_at = now(),
                reviewed_by_admin_id = $2
          WHERE id = $1 AND status = 'pending'
          RETURNING id",
    )
    .bind(id)
    .bind(admin_id)
    .fetch_optional(conn)
    .await
}

/// Reject by deletion. There is no 'rejected' status: keeping
/// unwanted user-submitted bytes is a liability, and the audit row
/// records that the rejection happened.
pub async fn delete(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar::<_, Uuid>("DELETE FROM sightings WHERE id = $1 RETURNING id")
        .bind(id)
        .fetch_optional(conn)
        .await
}
