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
