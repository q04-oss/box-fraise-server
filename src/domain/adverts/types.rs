use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// An advert as it sits unopened in an inbox.
///
/// No `body`. That is the whole point of the thing: what opening it
/// reveals is not sent to somebody who has not opened it.
#[derive(Serialize, FromRow)]
pub struct Unopened {
    pub id: Uuid,
    pub advertiser: String,
    pub teaser: String,
    pub pays_cents: i32,
}

/// The inbox in one request: what is waiting, and what it has earned.
#[derive(Serialize)]
pub struct Inbox {
    pub adverts: Vec<Unopened>,
    /// Opened and not yet handed over in cash.
    pub owed_cents: i64,
    /// Collected, all time.
    pub paid_cents: i64,
}

/// What comes back the moment somebody opens one.
#[derive(Serialize)]
pub struct Opened {
    pub advertiser: String,
    pub body: String,
    pub link: Option<String>,
    pub amount_cents: i32,
    pub owed_cents: i64,
}

#[derive(Deserialize)]
pub struct NewAdvert {
    pub advertiser: String,
    pub teaser: String,
    pub body: String,
    #[serde(default)]
    pub link: Option<String>,
    /// What the advertiser pays per open. Defaults to the standing
    /// price, so the common case is not typing it.
    #[serde(default)]
    pub price_cents: Option<i32>,
    /// What the reader receives. Defaults to the standing share.
    #[serde(default)]
    pub pays_cents: Option<i32>,
    pub opens_paid: i32,
}

/// What a business sends in when they outline an advertisement.
///
/// The only place on the running layer that collects a contact detail. A
/// runner gives no email and is never asked for one; a business has to
/// be reachable, because somebody has to invoice them.
#[derive(Deserialize)]
pub struct NewRequest {
    pub advertiser: String,
    pub contact: String,
    pub teaser: String,
    pub body: String,
    #[serde(default)]
    pub link: Option<String>,
    pub opens_wanted: i32,
}

#[derive(Serialize, FromRow)]
pub struct AdminRequest {
    pub id: Uuid,
    pub advertiser: String,
    pub contact: String,
    pub teaser: String,
    pub body: String,
    pub link: Option<String>,
    pub opens_wanted: i32,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize, FromRow)]
pub struct AdminAdvert {
    pub id: Uuid,
    /// So the panel can offer the ledger link to put in an invoice.
    pub advertiser_id: Uuid,
    pub advertiser: String,
    pub teaser: String,
    /// What the advertiser pays per open.
    pub price_cents: i32,
    /// What the reader receives per open. The difference is what the
    /// platform keeps, and both are stored so it can be told.
    pub pays_cents: i32,
    pub opens_paid: i32,
    pub opens_taken: i32,
    pub status: String,
}

/// Who to pay, for whoever is handing out cash at a run.
#[derive(Serialize, FromRow)]
pub struct Owed {
    pub username: String,
    pub owed_cents: i64,
    pub opens: i64,
}

#[derive(Deserialize)]
pub struct PayRequest {
    pub username: String,
}

#[derive(Serialize)]
pub struct Paid {
    pub username: String,
    pub amount_cents: i64,
    pub opens: i64,
}

/// One advert on a business's ledger.
#[derive(Serialize, FromRow)]
pub struct LedgerRow {
    #[serde(skip_serializing)]
    pub name: String,
    pub advert_id: Uuid,
    pub teaser: String,
    pub price_cents: i32,
    pub pays_cents: i32,
    pub opens_paid: i32,
    pub opens_taken: i32,
    pub status: String,
    pub paid_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// What a business sees when they open the link they were sent.
///
/// No account, no password. The address is the permission — see 0042.
#[derive(Serialize)]
pub struct Ledger {
    pub name: String,
    /// Everything invoiced, all time.
    pub spent_cents: i64,
    /// Opens bought, against opens actually taken.
    pub opens_bought: i64,
    pub opens_taken: i64,
    /// What has gone to readers out of that spend.
    pub to_readers_cents: i64,
    pub adverts: Vec<LedgerRow>,
}
