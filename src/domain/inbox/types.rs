use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// An offer as it sits in a member's inbox.
///
/// No amount of who-you-are went into deciding it is here. Every current
/// member sees the same open offers in the same order — see 0038.
#[derive(Serialize, FromRow)]
pub struct Offer {
    pub id: Uuid,
    /// Where the artwork comes from: `/v1/marks/{mark_id}/image`, the
    /// same endpoint the scanner and the game use.
    pub mark_id: Uuid,
    pub headline: String,
    pub amount_cents: i32,
    pub explicit: bool,
    /// The business that bought it. Null for the platform's own marks —
    /// the strawberry belongs to nobody.
    pub business_name: Option<String>,
    /// The mark's label, which is what to show when there is no business.
    pub label: String,
}

/// The whole inbox in one request: what is waiting, and what is owed.
///
/// Together because they are read together — the balance is the reason
/// the list is worth opening, and two round trips on a phone in a park
/// is one too many.
#[derive(Serialize)]
pub struct Inbox {
    pub offers: Vec<Offer>,
    /// Accepted and not yet handed over in cash.
    pub owed_cents: i64,
    /// Collected, all time. The number that says this is real.
    pub paid_cents: i64,
}

#[derive(Serialize)]
pub struct Accepted {
    pub amount_cents: i32,
    pub owed_cents: i64,
}

#[derive(Deserialize)]
pub struct NewOffer {
    pub mark_id: Uuid,
    pub headline: String,
    pub amount_cents: i32,
    pub views_paid: i32,
    #[serde(default)]
    pub explicit: bool,
}

#[derive(Serialize, FromRow)]
pub struct AdminOffer {
    pub id: Uuid,
    pub headline: String,
    pub amount_cents: i32,
    pub views_paid: i32,
    pub views_taken: i32,
    pub explicit: bool,
    pub status: String,
    pub business_name: Option<String>,
    pub label: String,
}

/// Who to pay, for an admin standing at a run with cash.
///
/// A member number and an amount, because that is what the conversation
/// is: somebody says their number and is handed money.
#[derive(Serialize, FromRow)]
pub struct Owed {
    pub member_no: i32,
    pub owed_cents: i64,
    pub views: i64,
}

#[derive(Deserialize)]
pub struct PayRequest {
    pub member_no: i32,
}

#[derive(Serialize)]
pub struct Paid {
    pub member_no: i32,
    pub amount_cents: i64,
    pub views: i64,
}
