use sqlx::PgConnection;
use uuid::Uuid;

use super::types::{PendingSubmission, PublishedSubmission, SubmissionImage};

/// The member write. `status` is not a parameter — it is hardcoded
/// 'pending' here so no request body can influence it. The RLS INSERT
/// policy independently rejects anything else; this is the first of
/// the two barriers.
///
/// The id is generated in Rust rather than read back with `RETURNING`.
/// Postgres applies SELECT policies to the row an `INSERT ...
/// RETURNING` produces, and `submissions` has no non-admin SELECT
/// policy at all — so RETURNING on this path fails with 42501 even
/// though the insert itself is allowed. Supplying the id means the
/// public write needs no read privilege whatsoever.
pub async fn insert_submission(
    conn: &mut PgConnection,
    user_id: Uuid,
    title: Option<&str>,
    body: Option<&str>,
    image: Option<(&[u8], &str)>,
    member_no: i32,
) -> sqlx::Result<Uuid> {
    let id = Uuid::new_v4();
    let (bytes, content_type) = match image {
        Some((b, c)) => (Some(b), Some(c)),
        None => (None, None),
    };
    sqlx::query(
        "INSERT INTO submissions
             (id, title, body, image_bytes, content_type, byte_size,
              member_no, user_id, status)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'pending')",
    )
    .bind(id)
    .bind(title)
    .bind(body)
    .bind(bytes)
    .bind(content_type)
    .bind(bytes.map(|b| b.len() as i32))
    .bind(member_no)
    .bind(user_id)
    .execute(conn)
    .await?;
    Ok(id)
}

/// Global pending count, via the SECURITY DEFINER function added in
/// migration 0018. A public transaction cannot see pending rows to
/// count them itself.
pub async fn pending_count(conn: &mut PgConnection) -> sqlx::Result<i32> {
    sqlx::query_scalar::<_, i32>("SELECT bf_pending_submission_count()")
        .fetch_one(conn)
        .await
}

/// The editor's queue, oldest first. Admin transaction only — there is
/// no other way to read this table.
pub async fn list_pending(conn: &mut PgConnection) -> sqlx::Result<Vec<PendingSubmission>> {
    sqlx::query_as::<_, PendingSubmission>(
        "SELECT id, title, body,
                (image_bytes IS NOT NULL) AS has_image,
                byte_size, member_no, submitted_at
           FROM submissions
          WHERE status = 'pending'
          ORDER BY submitted_at ASC",
    )
    .fetch_all(conn)
    .await
}

pub async fn image(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<Option<SubmissionImage>> {
    let row: Option<(Option<Vec<u8>>, Option<String>)> =
        sqlx::query_as("SELECT image_bytes, content_type FROM submissions WHERE id = $1")
            .bind(id)
            .fetch_optional(conn)
            .await?;
    Ok(match row {
        Some((Some(bytes), Some(content_type))) => Some(SubmissionImage {
            bytes,
            content_type,
        }),
        _ => None,
    })
}

/// Race-close: two admins accepting the same submission at once means
/// exactly one UPDATE touches a row, and the other gets zero.
pub async fn accept(conn: &mut PgConnection, admin_id: Uuid, id: Uuid) -> sqlx::Result<bool> {
    let done = sqlx::query(
        "UPDATE submissions
            SET status = 'accepted',
                reviewed_at = now(),
                reviewed_by_admin_id = $1
          WHERE id = $2 AND status = 'pending'",
    )
    .bind(admin_id)
    .bind(id)
    .execute(conn)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Rejection deletes. Keeping writing and photographs that will not be
/// used is a liability and a discourtesy; the audit row is the record
/// that the submission existed.
pub async fn delete(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<bool> {
    let done = sqlx::query("DELETE FROM submissions WHERE id = $1 AND status = 'pending'")
        .bind(id)
        .execute(conn)
        .await?;
    Ok(done.rows_affected() == 1)
}

/// The feed: published posts, newest first.
///
/// Columns are named rather than globbed. The `status` filter is explicit rather than left to RLS,
/// so the query means the same thing under an admin transaction as
/// under a public one — an admin reading the feed must not start seeing
/// pending rows in it.
pub async fn list_published(
    conn: &mut PgConnection,
    limit: i64,
) -> sqlx::Result<Vec<PublishedSubmission>> {
    sqlx::query_as::<_, PublishedSubmission>(
        "SELECT id, title, body, member_no,
                (image_bytes IS NOT NULL) AS has_image,
                reviewed_at AS published_at
           FROM submissions
          WHERE status = 'accepted' AND reviewed_at IS NOT NULL
          ORDER BY reviewed_at DESC
          LIMIT $1",
    )
    .bind(limit)
    .fetch_all(conn)
    .await
}

/// A published photograph. Accepted rows only, so an unreviewed image
/// can never be fetched by guessing an id.
pub async fn published_image(
    conn: &mut PgConnection,
    id: Uuid,
) -> sqlx::Result<Option<SubmissionImage>> {
    let row: Option<(Option<Vec<u8>>, Option<String>)> = sqlx::query_as(
        "SELECT image_bytes, content_type
           FROM submissions
          WHERE id = $1 AND status = 'accepted'",
    )
    .bind(id)
    .fetch_optional(conn)
    .await?;
    Ok(match row {
        Some((Some(bytes), Some(content_type))) => Some(SubmissionImage {
            bytes,
            content_type,
        }),
        _ => None,
    })
}
