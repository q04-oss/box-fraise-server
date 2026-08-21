use sqlx::PgConnection;
use uuid::Uuid;

use super::types::{AdminOffer, Offer, Owed};

/// What is still worth showing this member: open, still in budget, and
/// not already said yes to.
///
/// The `NOT EXISTS` reads only the member's own receipts, which is all
/// the SELECT policy on `ad_views` would return anyway. Oldest first —
/// an offer that has been waiting longest is the one closest to expiring
/// its budget, and a queue people work down beats a pile.
pub async fn open_for(conn: &mut PgConnection, user_id: Uuid) -> sqlx::Result<Vec<Offer>> {
    sqlx::query_as::<_, Offer>(
        "SELECT o.id, o.mark_id, o.headline, o.amount_cents, o.explicit,
                b.name AS business_name, m.label
           FROM ad_offers o
           JOIN marks m ON m.id = o.mark_id
           LEFT JOIN businesses b ON b.id = m.business_id
          WHERE o.status = 'open'
            AND o.views_taken < o.views_paid
            AND NOT EXISTS (
                  SELECT 1 FROM ad_views v
                   WHERE v.offer_id = o.id AND v.user_id = $1)
          ORDER BY o.created_at ASC",
    )
    .bind(user_id)
    .fetch_all(conn)
    .await
}

/// Owed and collected, in one pass. Both are sums over the member's own
/// rows; a stored balance would be a number that can drift from the rows
/// that justify it.
pub async fn balances(conn: &mut PgConnection, user_id: Uuid) -> sqlx::Result<(i64, i64)> {
    sqlx::query_as::<_, (i64, i64)>(
        "SELECT COALESCE(SUM(amount_cents) FILTER (WHERE paid_at IS NULL), 0)::bigint,
                COALESCE(SUM(amount_cents) FILTER (WHERE paid_at IS NOT NULL), 0)::bigint
           FROM ad_views
          WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_one(conn)
    .await
}

/// Say yes to one offer.
///
/// Through the SECURITY DEFINER function from 0038, which checks the
/// budget, moves the counter and writes the receipt in one statement's
/// worth of atomicity. `bf_app` has no INSERT on `ad_views` at all, and
/// the function reads the member from the transaction's GUC rather than
/// taking one — so there is no argument with which to accept on somebody
/// else's behalf.
///
/// `None` means the offer was closed, out of budget, already taken by
/// this member, or the membership has lapsed. The service does not
/// distinguish them and neither should a caller: all four mean the same
/// thing to somebody looking at a phone.
pub async fn accept(
    conn: &mut PgConnection,
    offer_id: Uuid,
    view_id: Uuid,
) -> sqlx::Result<Option<i32>> {
    sqlx::query_scalar("SELECT bf_accept_offer($1, $2)")
        .bind(offer_id)
        .bind(view_id)
        .fetch_one(conn)
        .await
}

pub async fn list_all(conn: &mut PgConnection) -> sqlx::Result<Vec<AdminOffer>> {
    sqlx::query_as::<_, AdminOffer>(
        "SELECT o.id, o.headline, o.amount_cents, o.views_paid, o.views_taken,
                o.explicit, o.status, b.name AS business_name, m.label
           FROM ad_offers o
           JOIN marks m ON m.id = o.mark_id
           LEFT JOIN businesses b ON b.id = m.business_id
          ORDER BY o.created_at DESC",
    )
    .fetch_all(conn)
    .await
}

pub async fn insert(
    conn: &mut PgConnection,
    mark_id: Uuid,
    headline: &str,
    amount_cents: i32,
    views_paid: i32,
    explicit: bool,
    admin_id: Uuid,
) -> sqlx::Result<Uuid> {
    sqlx::query_scalar(
        "INSERT INTO ad_offers
             (mark_id, headline, amount_cents, views_paid, explicit,
              created_by_admin_id)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING id",
    )
    .bind(mark_id)
    .bind(headline)
    .bind(amount_cents)
    .bind(views_paid)
    .bind(explicit)
    .bind(admin_id)
    .fetch_one(conn)
    .await
}

pub async fn close(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<bool> {
    let done = sqlx::query("UPDATE ad_offers SET status = 'closed' WHERE id = $1")
        .bind(id)
        .execute(conn)
        .await?;
    Ok(done.rows_affected() == 1)
}

/// The list an admin reads out at a run: every member with money
/// waiting, biggest first.
pub async fn owed(conn: &mut PgConnection) -> sqlx::Result<Vec<Owed>> {
    sqlx::query_as::<_, Owed>(
        "SELECT u.member_no,
                SUM(v.amount_cents)::bigint AS owed_cents,
                COUNT(*)::bigint            AS views
           FROM ad_views v
           JOIN users u ON u.id = v.user_id
          WHERE v.paid_at IS NULL AND u.member_no IS NOT NULL
          GROUP BY u.member_no
          ORDER BY owed_cents DESC",
    )
    .fetch_all(conn)
    .await
}

/// Hand over the money.
///
/// Everything unpaid for that member becomes paid at once, because that
/// is what happened: a person was handed the amount they were owed. The
/// return is what was settled, so the admin can be told what to count
/// out and the audit entry can say what left the tin.
pub async fn pay(
    conn: &mut PgConnection,
    member_no: i32,
    admin_id: Uuid,
) -> sqlx::Result<(i64, i64)> {
    sqlx::query_as::<_, (i64, i64)>(
        "WITH settled AS (
             UPDATE ad_views v
                SET paid_at = now(), paid_by_admin_id = $2
               FROM users u
              WHERE u.id = v.user_id
                AND u.member_no = $1
                AND v.paid_at IS NULL
          RETURNING v.amount_cents
         )
         SELECT COALESCE(SUM(amount_cents), 0)::bigint,
                COUNT(*)::bigint
           FROM settled",
    )
    .bind(member_no)
    .bind(admin_id)
    .fetch_one(conn)
    .await
}
