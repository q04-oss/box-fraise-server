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
