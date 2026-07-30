use chrono::NaiveDate;
use sqlx::PgConnection;
use uuid::Uuid;

use super::types::{PendingStickerPhoto, Sticker, StickerPhoto, StickerPhotoImage};

/// The column list every sticker read shares.
///
/// The `photo_count` subquery filters `status = 'approved'`
/// explicitly rather than leaning on RLS to hide pending rows. Under
/// a public transaction the two are equivalent, but the admin list
/// runs with `app.is_admin` set — where RLS would let pending rows
/// into the aggregate and make the admin's "found" count disagree
/// with the public one. Being explicit keeps the number meaning the
/// same thing in both contexts.
const STICKER_COLUMNS: &str = "
    s.id, s.slug, s.label, s.hint, s.latitude, s.longitude,
    s.placed_on, s.sort_order, s.published,
    (SELECT COUNT(*) FROM sticker_photos p
      WHERE p.sticker_id = s.id AND p.status = 'approved') AS photo_count,
    s.created_at
";

/// Public map: published pins only. RLS enforces the same thing; the
/// WHERE clause keeps it obvious at the call site.
pub async fn list_published(conn: &mut PgConnection) -> sqlx::Result<Vec<Sticker>> {
    sqlx::query_as::<_, Sticker>(&format!(
        "SELECT {STICKER_COLUMNS}
           FROM stickers s
          WHERE s.published = true
          ORDER BY s.sort_order DESC, s.label ASC"
    ))
    .fetch_all(conn)
    .await
}

/// Admin list: drafts included. Requires an AdminRlsTransaction —
/// without `app.is_admin` the SELECT policy filters unpublished rows
/// and this silently behaves like `list_published`.
pub async fn list_all(conn: &mut PgConnection) -> sqlx::Result<Vec<Sticker>> {
    sqlx::query_as::<_, Sticker>(&format!(
        "SELECT {STICKER_COLUMNS}
           FROM stickers s
          ORDER BY s.sort_order DESC, s.label ASC"
    ))
    .fetch_all(conn)
    .await
}

/// Single pin by slug. Published-only, so it is safe on public paths
/// and is what the submit path uses to resolve slug → id.
pub async fn get_published_by_slug(
    conn: &mut PgConnection,
    slug: &str,
) -> sqlx::Result<Option<Sticker>> {
    sqlx::query_as::<_, Sticker>(&format!(
        "SELECT {STICKER_COLUMNS}
           FROM stickers s
          WHERE s.slug = $1 AND s.published = true"
    ))
    .bind(slug)
    .fetch_optional(conn)
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_sticker(
    conn: &mut PgConnection,
    slug: &str,
    label: &str,
    hint: Option<&str>,
    latitude: f64,
    longitude: f64,
    placed_on: Option<NaiveDate>,
    sort_order: i32,
    published: bool,
) -> sqlx::Result<Sticker> {
    // A brand-new sticker has no photos, so the count is a literal
    // rather than a correlated subquery over the row we just wrote.
    sqlx::query_as::<_, Sticker>(
        "INSERT INTO stickers
             (slug, label, hint, latitude, longitude, placed_on, sort_order, published)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         RETURNING id, slug, label, hint, latitude, longitude, placed_on,
                   sort_order, published, 0::bigint AS photo_count, created_at",
    )
    .bind(slug)
    .bind(label)
    .bind(hint)
    .bind(latitude)
    .bind(longitude)
    .bind(placed_on)
    .bind(sort_order)
    .bind(published)
    .fetch_one(conn)
    .await
}

/// Public gallery for one pin. The `status` filter is explicit for
/// the same reason as in STICKER_COLUMNS.
pub async fn list_approved_photos(
    conn: &mut PgConnection,
    sticker_id: Uuid,
) -> sqlx::Result<Vec<StickerPhoto>> {
    sqlx::query_as::<_, StickerPhoto>(
        "SELECT id, caption, submitter_name, submitted_at
           FROM sticker_photos
          WHERE sticker_id = $1 AND status = 'approved'
          ORDER BY submitted_at DESC",
    )
    .bind(sticker_id)
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
/// row an `INSERT ... RETURNING` produces, and a pending photo is
/// deliberately invisible to `sticker_photos_public_select` — so
/// RETURNING on this path fails with a 42501 even though the insert
/// itself is allowed. Supplying the id means the public write needs no
/// read privilege at all, which is the property we actually want.
pub async fn insert_photo(
    conn: &mut PgConnection,
    sticker_id: Uuid,
    image_bytes: &[u8],
    content_type: &str,
    caption: Option<&str>,
    submitter_name: Option<&str>,
) -> sqlx::Result<Uuid> {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO sticker_photos
             (id, sticker_id, image_bytes, content_type, byte_size,
              caption, submitter_name, status)
         VALUES ($1, $2, $3, $4, $5, $6, $7, 'pending')",
    )
    .bind(id)
    .bind(sticker_id)
    .bind(image_bytes)
    .bind(content_type)
    .bind(image_bytes.len() as i32)
    .bind(caption)
    .bind(submitter_name)
    .execute(conn)
    .await?;
    Ok(id)
}

/// Pending count for one pin, via the SECURITY DEFINER function added
/// in migration 0012. A public transaction cannot see pending rows to
/// count them itself — see the function's comment for why this is a
/// function and not a plain SELECT.
pub async fn pending_photo_count(conn: &mut PgConnection, sticker_id: Uuid) -> sqlx::Result<i32> {
    sqlx::query_scalar::<_, i32>("SELECT bf_sticker_pending_photo_count($1)")
        .bind(sticker_id)
        .fetch_one(conn)
        .await
}

/// Image bytes. Visibility is entirely RLS-driven: on a plain
/// transaction only approved photos resolve, so the public endpoint
/// cannot leak an unmoderated image. The admin endpoint runs the same
/// query under `app.is_admin` to preview the queue.
pub async fn get_photo_image(
    conn: &mut PgConnection,
    photo_id: Uuid,
) -> sqlx::Result<Option<StickerPhotoImage>> {
    sqlx::query_as::<_, StickerPhotoImage>(
        "SELECT image_bytes, content_type FROM sticker_photos WHERE id = $1",
    )
    .bind(photo_id)
    .fetch_optional(conn)
    .await
}

/// Moderation queue, oldest first so submissions are reviewed in
/// arrival order.
pub async fn list_pending_photos(
    conn: &mut PgConnection,
) -> sqlx::Result<Vec<PendingStickerPhoto>> {
    sqlx::query_as::<_, PendingStickerPhoto>(
        "SELECT p.id, p.sticker_id, s.slug AS sticker_slug, s.label AS sticker_label,
                p.caption, p.submitter_name, p.content_type, p.byte_size, p.submitted_at
           FROM sticker_photos p
           JOIN stickers s ON s.id = p.sticker_id
          WHERE p.status = 'pending'
          ORDER BY p.submitted_at ASC",
    )
    .fetch_all(conn)
    .await
}

/// Approve. `AND status = 'pending'` is the race-close, the same
/// idiom as the verify flip: two admins approving the same photo
/// concurrently produce exactly one row, and the loser gets None →
/// 409 rather than a second audit entry.
pub async fn approve_photo(
    conn: &mut PgConnection,
    photo_id: Uuid,
    admin_id: Uuid,
) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar::<_, Uuid>(
        "UPDATE sticker_photos
            SET status = 'approved',
                reviewed_at = now(),
                reviewed_by_admin_id = $2
          WHERE id = $1 AND status = 'pending'
          RETURNING sticker_id",
    )
    .bind(photo_id)
    .bind(admin_id)
    .fetch_optional(conn)
    .await
}

/// Reject by deletion. There is no 'rejected' status: keeping
/// unwanted user-submitted bytes is a liability, and the audit row
/// records that the rejection happened. Returns the parent sticker id
/// for that audit entry, or None if the photo was already gone.
pub async fn delete_photo(conn: &mut PgConnection, photo_id: Uuid) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar::<_, Uuid>("DELETE FROM sticker_photos WHERE id = $1 RETURNING sticker_id")
        .bind(photo_id)
        .fetch_optional(conn)
        .await
}
