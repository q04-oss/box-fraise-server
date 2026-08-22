//! The inbox: adverts a signed-in runner chooses to open, and is paid
//! for opening.
//!
//! Three absences are the design, and each is a thing somebody will
//! eventually propose adding:
//!
//! **No targeting.** An advert is not addressed to anybody. Every
//! signed-in runner sees the same open adverts in the same order, oldest
//! first. There is no scoring function here and there should not be one.
//!
//! **No measurement of attention.** No dwell timer, no scroll depth, no
//! read receipt. Choosing to open it is the transaction being paid for;
//! checking whether somebody really read it would be surveillance
//! wrapped around the one consensual thing in the product.
//!
//! **No payment processing.** `pays_cents * opens_paid` changed hands
//! before the advert row existed, and what a runner earns is handed over
//! as cash in person. Nothing in this module talks to a card network.

use uuid::Uuid;

use super::{
    repository,
    types::{AdminAdvert, Inbox, NewAdvert, Opened, Owed, Paid, PayRequest},
};
use crate::{
    audit,
    db::{AdminRlsTransaction, Pool, RunnerRlsTransaction},
    error::{AppError, AppResult},
};

/// $100 an open, mirroring `adverts_pays_sane`. Not a view about what
/// attention is worth — a guard against a typo becoming a debt.
const MAX_PAYS_CENTS: i32 = 10_000;
const MAX_OPENS: i32 = 100_000;

pub async fn inbox(pool: &Pool, runner_id: Uuid) -> AppResult<Inbox> {
    let mut tx = RunnerRlsTransaction::begin(pool, runner_id).await?;
    let adverts = repository::unopened_for(tx.conn(), runner_id).await?;
    let (owed_cents, paid_cents) = repository::balances(tx.conn(), runner_id).await?;
    tx.commit().await?;
    Ok(Inbox {
        adverts,
        owed_cents,
        paid_cents,
    })
}

/// Open one, and be paid for it.
///
/// The contents are read only after the open succeeded, in the same
/// transaction. Nothing returns `body` on any other path.
pub async fn open(pool: &Pool, runner_id: Uuid, advert_id: Uuid) -> AppResult<Opened> {
    let mut tx = RunnerRlsTransaction::begin(pool, runner_id).await?;
    let amount = repository::open(tx.conn(), advert_id, Uuid::new_v4()).await?;
    let Some(amount_cents) = amount else {
        tx.commit().await?;
        // Closed, spent, or already opened. One message for all three:
        // they mean the same thing to somebody holding a phone, and
        // saying which would tell a caller about adverts they cannot
        // see.
        return Err(AppError::bad_request("that one is no longer open"));
    };
    let contents = repository::contents(tx.conn(), advert_id).await?;
    let (owed_cents, _) = repository::balances(tx.conn(), runner_id).await?;
    tx.commit().await?;

    audit::write(
        pool,
        "public",
        None,
        "advert.opened",
        Some(&advert_id.to_string()),
        serde_json::json!({ "amount_cents": amount_cents }),
    )
    .await;

    Ok(repository::opened(contents, amount_cents, owed_cents))
}

pub async fn list_all(pool: &Pool) -> AppResult<Vec<AdminAdvert>> {
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let rows = repository::list_all(tx.conn()).await?;
    tx.commit().await?;
    Ok(rows)
}

/// Put an advert in every inbox, after somebody paid for it.
pub async fn create(pool: &Pool, admin_id: Uuid, req: NewAdvert) -> AppResult<Uuid> {
    let advertiser = req.advertiser.trim();
    let teaser = req.teaser.trim();
    let body = req.body.trim();
    if advertiser.is_empty() || teaser.is_empty() || body.is_empty() {
        return Err(AppError::bad_request("a name, a line, and something inside it"));
    }
    if teaser.chars().count() > 140 {
        return Err(AppError::bad_request("that line is too long for an inbox"));
    }
    if body.chars().count() > 4000 {
        return Err(AppError::bad_request("that is too much to put inside one"));
    }
    if req.pays_cents <= 0 || req.pays_cents > MAX_PAYS_CENTS {
        return Err(AppError::bad_request("the amount is outside what an advert may pay"));
    }
    if req.opens_paid <= 0 || req.opens_paid > MAX_OPENS {
        return Err(AppError::bad_request("that is not a number of opens"));
    }
    let link = req.link.as_deref().map(str::trim).filter(|l| !l.is_empty());

    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let id = repository::insert(
        tx.conn(),
        advertiser,
        teaser,
        body,
        link,
        req.pays_cents,
        req.opens_paid,
    )
    .await?;
    tx.commit().await?;

    audit::write(
        pool,
        "admin",
        Some(admin_id),
        "advert.created",
        Some(&id.to_string()),
        serde_json::json!({ "pays_cents": req.pays_cents, "opens_paid": req.opens_paid }),
    )
    .await;

    Ok(id)
}

/// Stop it appearing. Receipts already written stay owed — what somebody
/// agreed to and what they are due are not the advertiser's to withdraw.
pub async fn close(pool: &Pool, admin_id: Uuid, id: Uuid) -> AppResult<()> {
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let done = repository::close(tx.conn(), id).await?;
    tx.commit().await?;
    if !done {
        return Err(AppError::NotFound);
    }
    audit::write(
        pool,
        "admin",
        Some(admin_id),
        "advert.closed",
        Some(&id.to_string()),
        serde_json::json!({}),
    )
    .await;
    Ok(())
}

pub async fn owed(pool: &Pool) -> AppResult<Vec<Owed>> {
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let rows = repository::owed(tx.conn()).await?;
    tx.commit().await?;
    Ok(rows)
}

/// Record that somebody was handed what they were owed. Nothing here
/// moves money — the money moved across a hand, and this is the note.
pub async fn pay(pool: &Pool, admin_id: Uuid, req: PayRequest) -> AppResult<Paid> {
    let username = req.username.trim().to_lowercase();
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let (amount_cents, opens) = repository::pay(tx.conn(), &username).await?;
    tx.commit().await?;

    if opens == 0 {
        return Err(AppError::bad_request("nothing is owed to that name"));
    }

    audit::write(
        pool,
        "admin",
        Some(admin_id),
        "advert.paid",
        Some(&username),
        serde_json::json!({ "amount_cents": amount_cents, "opens": opens }),
    )
    .await;

    Ok(Paid {
        username,
        amount_cents,
        opens,
    })
}
