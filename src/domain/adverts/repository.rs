use sqlx::PgConnection;
use uuid::Uuid;

use super::types::{AdminAdvert, AdminRequest, LedgerRow, Opened, Owed, Unopened};

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
            AND NOT EXISTS (
                  SELECT 1 FROM ad_declines d
                   WHERE d.advert_id = a.id AND d.runner_id = $1)
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

/// How many opens are left today, and what the ceiling is.
///
/// Both read from the database rather than a constant in Rust, so what
/// a reader is told and what `bf_open_advert` will actually permit
/// cannot disagree.
pub async fn allowance(conn: &mut PgConnection, runner_id: Uuid) -> sqlx::Result<(i32, i32)> {
    sqlx::query_as::<_, (i32, i32)>(
        "SELECT GREATEST(0, bf_daily_open_limit() - bf_opens_today($1)),
                bf_daily_open_limit()",
    )
    .bind(runner_id)
    .fetch_one(conn)
    .await
}

/// Say no. No function and no counter: refusing writes one row the
/// runner owns, which an ordinary policy covers, and it costs the
/// advertiser nothing. See 0043.
pub async fn decline(
    conn: &mut PgConnection,
    id: Uuid,
    advert_id: Uuid,
    runner_id: Uuid,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO ad_declines (id, advert_id, runner_id)
         VALUES ($1, $2, $3)
         ON CONFLICT (advert_id, runner_id) DO NOTHING",
    )
    .bind(id)
    .bind(advert_id)
    .bind(runner_id)
    .execute(conn)
    .await?;
    Ok(())
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
        "SELECT id, advertiser_id, advertiser, teaser, price_cents, pays_cents,
                opens_paid, opens_taken, status
           FROM adverts
          ORDER BY created_at DESC",
    )
    .fetch_all(conn)
    .await
}

/// Find the business by name, or make one.
///
/// Through the SECURITY DEFINER function from 0042 so that matching is
/// done in one statement rather than a read-then-write that races with
/// itself. Lowercased name is the key, so two adverts typed months apart
/// still total together.
pub async fn advertiser_for(
    conn: &mut PgConnection,
    name: &str,
    contact: &str,
) -> sqlx::Result<Uuid> {
    sqlx::query_scalar("SELECT bf_advertiser_for($1, $2)")
        .bind(name)
        .bind(contact)
        .fetch_one(conn)
        .await
}

/// `paid_at` is set here because an advert only goes up once the invoice
/// has been settled. If that ever stops being true, this is the line
/// that has to change rather than a comment somewhere.
#[allow(clippy::too_many_arguments)]
pub async fn insert(
    conn: &mut PgConnection,
    advertiser_id: Uuid,
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
             (advertiser_id, advertiser, teaser, body, link,
              price_cents, pays_cents, opens_paid, paid_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, now())
         RETURNING id",
    )
    .bind(advertiser_id)
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

/// One business's adverts, by id and nothing else.
///
/// Through the SECURITY DEFINER function from 0042: the page that calls
/// this has no session, because a business is sent a link rather than
/// given an account. The parameter is the whole permission — there is no
/// way to ask for a list and no way to walk from one advertiser to
/// another.
pub async fn ledger(conn: &mut PgConnection, advertiser: Uuid) -> sqlx::Result<Vec<LedgerRow>> {
    sqlx::query_as::<_, LedgerRow>("SELECT * FROM bf_advertiser_ledger($1)")
        .bind(advertiser)
        .fetch_all(conn)
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
