//! The inbox: a business asks, a member decides, and the member is paid.
//!
//! This is the transaction the whole platform is an argument for, so it
//! is worth naming what is deliberately absent from it.
//!
//! Nothing here decides *who* sees an offer. There is no targeting, no
//! ranking and no profile. Every current member sees the same open
//! offers, oldest first, and the only thing that removes one from a list
//! is that the member already said yes or the budget ran out.
//!
//! Nothing here measures whether anybody looked. Saying yes is the
//! product — a person choosing to give attention to a named business —
//! and a dwell timer bolted onto it would be surveillance wrapped around
//! the one consensual thing on the site.
//!
//! And nothing here takes a payment. Money arrives as cash at a run
//! before an offer exists, and leaves as cash at a run when an admin
//! marks a member paid. See /cash for why that is the point rather than
//! a stage the platform has not reached yet.

use uuid::Uuid;

use super::{
    repository,
    types::{Accepted, AdminOffer, Inbox, NewOffer, Owed, Paid, PayRequest},
};
use crate::{
    audit,
    db::{AdminRlsTransaction, Pool, RlsTransaction},
    error::{AppError, AppResult},
};

/// $100 a view, mirroring `ad_offers_amount_sane`. Not a judgement about
/// what attention is worth — a guard against a typo becoming a debt.
const MAX_AMOUNT_CENTS: i32 = 10_000;
const MAX_VIEWS: i32 = 100_000;
const MAX_HEADLINE: usize = 140;

pub async fn inbox(pool: &Pool, user_id: Uuid) -> AppResult<Inbox> {
    let mut tx = RlsTransaction::begin(pool, user_id).await?;
    let offers = repository::open_for(tx.conn(), user_id).await?;
    let (owed_cents, paid_cents) = repository::balances(tx.conn(), user_id).await?;
    tx.commit().await?;
    Ok(Inbox {
        offers,
        owed_cents,
        paid_cents,
    })
}

/// Say yes.
///
/// The id is generated here rather than read back, the same as
/// submissions: `bf_app` has no INSERT on `ad_views`, so there is no
/// statement whose `RETURNING` could hand one over.
pub async fn accept(pool: &Pool, user_id: Uuid, offer_id: Uuid) -> AppResult<Accepted> {
    let mut tx = RlsTransaction::begin(pool, user_id).await?;
    let amount = repository::accept(tx.conn(), offer_id, Uuid::new_v4()).await?;
    let Some(amount_cents) = amount else {
        // Closed, out of budget, already taken, or a lapsed membership.
        // One message for all four: they mean the same thing to somebody
        // holding a phone, and saying which one would tell a caller
        // things about offers they cannot see.
        tx.commit().await?;
        return Err(AppError::bad_request("that offer is no longer open"));
    };
    let (owed_cents, _) = repository::balances(tx.conn(), user_id).await?;
    tx.commit().await?;

    audit::write(
        pool,
        "user",
        Some(user_id),
        "offer.accepted",
        Some(&offer_id.to_string()),
        serde_json::json!({ "amount_cents": amount_cents }),
    )
    .await;

    Ok(Accepted {
        amount_cents,
        owed_cents,
    })
}

pub async fn list_all(pool: &Pool) -> AppResult<Vec<AdminOffer>> {
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let rows = repository::list_all(tx.conn()).await?;
    tx.commit().await?;
    Ok(rows)
}

/// Create an offer, after a business has handed over cash at a run.
///
/// `amount_cents * views_paid` is money that already changed hands. This
/// endpoint records what it bought; it does not charge anybody.
pub async fn create(pool: &Pool, admin_id: Uuid, req: NewOffer) -> AppResult<Uuid> {
    let headline = req.headline.trim();
    if headline.is_empty() {
        return Err(AppError::bad_request("an offer needs a sentence"));
    }
    if headline.chars().count() > MAX_HEADLINE {
        return Err(AppError::bad_request("that sentence is too long for a phone"));
    }
    if req.amount_cents <= 0 || req.amount_cents > MAX_AMOUNT_CENTS {
        return Err(AppError::bad_request("the amount is outside what an offer may pay"));
    }
    if req.views_paid <= 0 || req.views_paid > MAX_VIEWS {
        return Err(AppError::bad_request("that is not a number of views"));
    }

    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let id = repository::insert(
        tx.conn(),
        req.mark_id,
        headline,
        req.amount_cents,
        req.views_paid,
        req.explicit,
        admin_id,
    )
    .await?;
    tx.commit().await?;

    audit::write(
        pool,
        "admin",
        Some(admin_id),
        "offer.created",
        Some(&id.to_string()),
        serde_json::json!({
            "amount_cents": req.amount_cents,
            "views_paid": req.views_paid,
            "explicit": req.explicit,
        }),
    )
    .await;

    Ok(id)
}

/// Stop an offer appearing. Receipts already written stay owed — what
/// somebody agreed to and what they are due are not the advertiser's to
/// withdraw.
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
        "offer.closed",
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

/// Record that a member was handed what they were owed.
///
/// The same shape as attendance: an admin types a member's number while
/// looking at them. Nothing in this call moves money — the money moved
/// across a hand, and this is the note about it.
pub async fn pay(pool: &Pool, admin_id: Uuid, req: PayRequest) -> AppResult<Paid> {
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let (amount_cents, views) = repository::pay(tx.conn(), req.member_no, admin_id).await?;
    tx.commit().await?;

    if views == 0 {
        return Err(AppError::bad_request("nothing is owed to that number"));
    }

    audit::write(
        pool,
        "admin",
        Some(admin_id),
        "offer.paid",
        Some(&req.member_no.to_string()),
        serde_json::json!({ "amount_cents": amount_cents, "views": views }),
    )
    .await;

    Ok(Paid {
        member_no: req.member_no,
        amount_cents,
        views,
    })
}

pub use super::types::PayRequest as PayRequestNo;
