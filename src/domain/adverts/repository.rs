use sqlx::PgConnection;
use uuid::Uuid;

use super::types::{AdminAdvert, AdminRequest, Opened, Owed, Unopened};

/// What is still worth showing this runner: open, in budget, and not
/// already opened by them.
///
/// **Never selects `body`.** Row-level security cannot hide a column, so
/// the policy that lets a runner see the queue would also let a query
/// read what is inside an unopened advert. The column list is the
/// boundary — do not add `body` here, and do not `SELECT *`.
pub async fn unopened_for(conn: &mut PgConnection, runner_id: Uuid) -> sqlx::Result<Vec<Unopened>> {
    sqlx::query_as::<_, Unopened>(
        "SELECT a.id, a.advertiser, a.teaser, a.pays_cents
           FROM adverts a
          WHERE a.status = 'open'
            AND a.opens_taken < a.opens_paid
            AND NOT EXISTS (
                  SELECT 1 FROM ad_opens o
                   WHERE o.advert_id = a.id AND o.runner_id = $1)
          ORDER BY a.created_at ASC",
    )
    .bind(runner_id)
    .fetch_all(conn)
    .await
}

/// Owed and collected, in one pass over the runner's own receipts.
pub async fn balances(conn: &mut PgConnection, runner_id: Uuid) -> sqlx::Result<(i64, i64)> {
    sqlx::query_as::<_, (i64, i64)>(
        "SELECT COALESCE(SUM(amount_cents) FILTER (WHERE paid_at IS NULL), 0)::bigint,
                COALESCE(SUM(amount_cents) FILTER (WHERE paid_at IS NOT NULL), 0)::bigint
           FROM ad_opens
          WHERE runner_id = $1",
    )
    .bind(runner_id)
    .fetch_one(conn)
    .await
}

/// Choose to open one.
///
/// Through the SECURITY DEFINER function from 0040, which checks the
/// budget, moves the counter and writes the receipt together. `bf_app`
/// has no INSERT on `ad_opens` at all, and the function reads the runner
/// from the transaction's GUC rather than taking one — so there is no
/// argument with which to open something as somebody else.
///
/// `None` means closed, out of budget, or already opened. The service
/// does not distinguish them: all three mean the same thing to somebody
/// looking at a phone.
pub async fn open(
    conn: &mut PgConnection,
    advert_id: Uuid,
    open_id: Uuid,
) -> sqlx::Result<Option<i32>> {
    sqlx::query_scalar("SELECT bf_open_advert($1, $2)")
        .bind(advert_id)
        .bind(open_id)
        .fetch_one(conn)
        .await
}

/// The contents, read only after the open above succeeded.
pub async fn contents(
    conn: &mut PgConnection,
    advert_id: Uuid,
) -> sqlx::Result<(String, String, Option<String>)> {
    sqlx::query_as("SELECT advertiser, body, link FROM adverts WHERE id = $1")
        .bind(advert_id)
        .fetch_one(conn)
        .await
}

pub async fn list_all(conn: &mut PgConnection) -> sqlx::Result<Vec<AdminAdvert>> {
    sqlx::query_as::<_, AdminAdvert>(
        "SELECT id, advertiser, teaser, price_cents, pays_cents,
                opens_paid, opens_taken, status
           FROM adverts
          ORDER BY created_at DESC",
    )
    .fetch_all(conn)
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn insert(
    conn: &mut PgConnection,
    advertiser: &str,
    teaser: &str,
    body: &str,
    link: Option<&str>,
    price_cents: i32,
    pays_cents: i32,
    opens_paid: i32,
) -> sqlx::Result<Uuid> {
    sqlx::query_scalar(
        "INSERT INTO adverts
             (advertiser, teaser, body, link, price_cents, pays_cents, opens_paid)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING id",
    )
    .bind(advertiser)
    .bind(teaser)
    .bind(body)
    .bind(link)
    .bind(price_cents)
    .bind(pays_cents)
    .bind(opens_paid)
    .fetch_one(conn)
    .await
}

/// A business outlining an advertisement they have.
///
/// The id is generated in Rust rather than read back. There is no
/// non-admin SELECT policy on this table, so `INSERT ... RETURNING`
/// would be refused with 42501 — the same reason `submissions` does it
/// this way. Do not widen the policy to make `RETURNING` work.
#[allow(clippy::too_many_arguments)]
pub async fn insert_request(
    conn: &mut PgConnection,
    id: Uuid,
    advertiser: &str,
    contact: &str,
    teaser: &str,
    body: &str,
    link: Option<&str>,
    opens_wanted: i32,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO advert_requests
             (id, advertiser, contact, teaser, body, link, opens_wanted)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(id)
    .bind(advertiser)
    .bind(contact)
    .bind(teaser)
    .bind(body)
    .bind(link)
    .bind(opens_wanted)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn list_requests(conn: &mut PgConnection) -> sqlx::Result<Vec<AdminRequest>> {
    sqlx::query_as::<_, AdminRequest>(
        "SELECT id, advertiser, contact, teaser, body, link,
                opens_wanted, status, created_at
           FROM advert_requests
          ORDER BY status DESC, created_at ASC",
    )
    .fetch_all(conn)
    .await
}

pub async fn take_request(
    conn: &mut PgConnection,
    id: Uuid,
) -> sqlx::Result<Option<AdminRequest>> {
    // The race-close: two admins accepting at once means exactly one
    // UPDATE returns a row, so an advert is created once.
    sqlx::query_as::<_, AdminRequest>(
        "UPDATE advert_requests
            SET status = 'accepted'
          WHERE id = $1 AND status = 'pending'
      RETURNING id, advertiser, contact, teaser, body, link,
                opens_wanted, status, created_at",
    )
    .bind(id)
    .fetch_optional(conn)
    .await
}

pub async fn delete_request(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<bool> {
    let done = sqlx::query("DELETE FROM advert_requests WHERE id = $1")
        .bind(id)
        .execute(conn)
        .await?;
    Ok(done.rows_affected() == 1)
}

pub async fn close(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<bool> {
    let done = sqlx::query("UPDATE adverts SET status = 'closed' WHERE id = $1")
        .bind(id)
        .execute(conn)
        .await?;
    Ok(done.rows_affected() == 1)
}

/// Everyone with money waiting, biggest first.
pub async fn owed(conn: &mut PgConnection) -> sqlx::Result<Vec<Owed>> {
    sqlx::query_as::<_, Owed>(
        "SELECT r.username,
                SUM(o.amount_cents)::bigint AS owed_cents,
                COUNT(*)::bigint            AS opens
           FROM ad_opens o
           JOIN runners r ON r.id = o.runner_id
          WHERE o.paid_at IS NULL
          GROUP BY r.username
          ORDER BY owed_cents DESC",
    )
    .fetch_all(conn)
    .await
}

/// Everything unpaid for that runner becomes paid at once, because that
/// is what happened: a person was handed what they were owed.
pub async fn pay(conn: &mut PgConnection, username: &str) -> sqlx::Result<(i64, i64)> {
    sqlx::query_as::<_, (i64, i64)>(
        "WITH settled AS (
             UPDATE ad_opens o
                SET paid_at = now()
               FROM runners r
              WHERE r.id = o.runner_id
                AND r.username = $1
                AND o.paid_at IS NULL
          RETURNING o.amount_cents
         )
         SELECT COALESCE(SUM(amount_cents), 0)::bigint, COUNT(*)::bigint FROM settled",
    )
    .bind(username)
    .fetch_one(conn)
    .await
}

/// Used only to build the reply after an open, so the page can print
/// what was revealed alongside what it paid.
pub fn opened(
    (advertiser, body, link): (String, String, Option<String>),
    amount_cents: i32,
    owed_cents: i64,
) -> Opened {
    Opened {
        advertiser,
        body,
        link,
        amount_cents,
        owed_cents,
    }
}
