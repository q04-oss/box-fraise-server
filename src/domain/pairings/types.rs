use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// What a caller sees about one of their own pairings.
///
/// **This type is the silent-decline boundary.** It carries the
/// caller's own decision and nothing about the other side's. There is
/// deliberately no field for it, no optional, no "pending" variant
/// that could be set from the counterparty's column — the shape makes
/// the leak unrepresentable rather than merely avoided.
///
/// If you are adding a field here, ask whether it could differ
/// depending on what the other person chose. If it could, it does not
/// belong.
#[derive(Debug, Clone, Serialize)]
pub struct PairingView {
    pub id: Uuid,
    /// `waiting` | `deciding` | `open` | `lapsed` | `closed`.
    /// Computed from timestamps, never stored, so it cannot drift.
    pub status: String,
    /// The other person. Their display name is all that crosses.
    pub peer_name: Option<String>,
    /// Only populated once the pairing is open — before that, a
    /// pairing is a memory of meeting someone, not a contact.
    pub peer_id: Option<Uuid>,
    pub event_name: Option<String>,
    pub met_at: DateTime<Utc>,
    pub opens_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    /// The caller's own answer: `null` until they give one.
    pub my_decision: Option<String>,
}

/// Raw row. Internal only — never serialize this.
#[derive(Debug, sqlx::FromRow)]
pub struct PairingRow {
    pub id: Uuid,
    pub lower_user_id: Uuid,
    pub upper_user_id: Uuid,
    pub lower_decision: Option<String>,
    pub upper_decision: Option<String>,
    pub met_at: DateTime<Utc>,
    pub opens_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub opened_at: Option<DateTime<Utc>>,
    pub closed_at: Option<DateTime<Utc>>,
    pub peer_name: Option<String>,
    pub event_name: Option<String>,
}

impl PairingRow {
    pub fn is_lower(&self, me: Uuid) -> bool {
        self.lower_user_id == me
    }

    pub fn peer_of(&self, me: Uuid) -> Uuid {
        if self.is_lower(me) {
            self.upper_user_id
        } else {
            self.lower_user_id
        }
    }

    pub fn my_decision(&self, me: Uuid) -> Option<String> {
        if self.is_lower(me) {
            self.lower_decision.clone()
        } else {
            self.upper_decision.clone()
        }
    }

    /// Status as the *caller* sees it. Note it never varies with the
    /// counterparty's decision: a pairing the other person declined
    /// and one they ignored are both `deciding`, then both `lapsed`,
    /// at exactly the same moments.
    pub fn status(&self, now: DateTime<Utc>) -> &'static str {
        if self.closed_at.is_some() {
            "closed"
        } else if self.opened_at.is_some() {
            "open"
        } else if now < self.opens_at {
            "waiting"
        } else if now >= self.expires_at {
            "lapsed"
        } else {
            "deciding"
        }
    }

    pub fn to_view(&self, me: Uuid, now: DateTime<Utc>) -> PairingView {
        let status = self.status(now);
        PairingView {
            id: self.id,
            status: status.to_string(),
            peer_name: self.peer_name.clone(),
            // Withheld until the channel exists. Before that there is
            // nothing to address, and a user id is the one thing that
            // would let someone bypass the gate downstream.
            peer_id: (status == "open").then(|| self.peer_of(me)),
            event_name: self.event_name.clone(),
            met_at: self.met_at,
            opens_at: self.opens_at,
            expires_at: self.expires_at,
            my_decision: self.my_decision(me),
        }
    }
}

/// What the initiator's screen shows as a QR. The nonce alone —
/// putting the user id in here would leak identity to anyone who
/// photographs the screen.
#[derive(Debug, Serialize)]
pub struct PairingNonceResponse {
    pub nonce: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct ClaimRequest {
    pub nonce: String,
    /// The scanner's signature over the nonce, from their device key.
    pub signature_b64: String,
}

#[derive(Debug, Serialize)]
pub struct ClaimResponse {
    pub pairing_id: Uuid,
    pub opens_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct DecisionRequest {
    /// `yes` or `no`.
    pub decision: String,
}

#[derive(Debug, Serialize)]
pub struct AuthorizedResponse {
    pub authorized: bool,
}

#[derive(Debug, Deserialize)]
pub struct SetDisplayNameRequest {
    pub display_name: String,
}
