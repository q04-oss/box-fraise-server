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
    pub pays_cents: i32,
    pub opens_paid: i32,
}

#[derive(Serialize, FromRow)]
pub struct AdminAdvert {
    pub id: Uuid,
    pub advertiser: String,
    pub teaser: String,
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
