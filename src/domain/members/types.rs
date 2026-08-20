use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Deserialize)]
pub struct CreateMemberRequest {
    /// The run this person turned up to. Required: the users table
    /// will not accept a verified row without an event and an admin,
    /// so a membership always records where it was granted.
    pub event_id: Uuid,
}

/// Returned once, at the moment of signing somebody up, and never
/// again — the server keeps only the hash. The admin's screen turns
/// `token` into a QR for the new member to scan.
#[derive(Serialize)]
pub struct CreatedMember {
    pub user_id: Uuid,
    /// What goes on their posts. Sequential, in the order people
    /// joined — nothing about a member is chosen by them.
    pub member_no: i32,
    pub token: String,
}

#[derive(Deserialize)]
pub struct RecordAttendanceRequest {
    pub event_id: Uuid,
    /// Asked for out loud and typed in. The admin is looking at the
    /// person, which is the only verification this needs.
    pub member_no: i32,
}

#[derive(Serialize)]
pub struct AttendanceRecorded {
    pub member_no: i32,
    /// False when this member was already marked present at this run —
    /// an admin pressing the button twice, not a second attendance.
    pub was_new: bool,
    pub current_until: DateTime<Utc>,
}

#[derive(Deserialize)]
pub struct ReissueRequest {
    /// Read off a screen, or off the person, or looked up in the
    /// attendance panel. The admin is standing in front of them.
    pub member_no: i32,
}

/// A replacement credential for a member who already exists.
///
/// The number is deliberately the old one. This is the whole point of
/// the endpoint: a lost phone should not cost somebody their byline,
/// their posts, or the year of turning up behind them.
#[derive(Serialize)]
pub struct ReissuedCredential {
    pub user_id: Uuid,
    pub member_no: i32,
    pub token: String,
    /// How many devices were signed out to make this one work. Shown to
    /// the admin so they can say it out loud — "your old phone is dead
    /// now" is better heard than discovered.
    pub sessions_ended: u64,
    /// Whether this member may currently post. False means they have
    /// not been at a run in over a month, and since they are standing
    /// right there, the admin should mark them present too.
    pub current: bool,
    pub last_seen: Option<DateTime<Utc>>,
}

/// What a member can see about their own standing.
#[derive(Serialize)]
pub struct MembershipStatus {
    pub member_no: i32,
    /// Whether they may post. False is not a lock-out: the account and
    /// everything they wrote stay exactly where they are.
    pub current: bool,
    pub last_seen: Option<DateTime<Utc>>,
    pub current_until: Option<DateTime<Utc>>,
}

/// The pitch, in numbers. Read by an admin standing in a cafe.
#[derive(Serialize)]
pub struct Reach {
    /// Distinct people who turned up in the last 30 days. This is the
    /// figure an advertisement reaches — not attendances, because
    /// somebody who came eight times is one person.
    pub people_30d: i64,
    pub people_90d: i64,
    /// Everybody who has ever been given a number.
    pub members_all_time: i64,
    pub runs: Vec<RunCount>,
}

#[derive(Serialize)]
pub struct RunCount {
    pub name: String,
    pub starts_at: DateTime<Utc>,
    pub turned_up: i64,
}
