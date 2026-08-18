//! Memberships, granted in person.
//!
//! There is one way to become a member of this platform: turn up to
//! the run club and have an admin make you an account while you are
//! standing there. No email, no link in an inbox, nothing that can
//! happen without somebody being in the room.
//!
//! The credential is handed over the same way — the admin's screen
//! shows a QR, the phone scans it and keeps the token. The server
//! keeps only its hash, so a membership shown once and not scanned is
//! a membership lost, and the admin makes another.
//!
//! A member has a number and no name. Nothing public about them is
//! chosen by them.

use serde_json::json;
use uuid::Uuid;

use super::{
    repository,
    types::{CreateMemberRequest, CreatedMember},
};
use crate::{
    audit, crypto,
    db::{AdminRlsTransaction, Pool},
    error::AppResult,
};

pub async fn create(
    pool: &Pool,
    admin_id: Uuid,
    req: CreateMemberRequest,
) -> AppResult<CreatedMember> {
    let (token, token_hash) = crypto::new_session_token();

    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let (user_id, member_no) =
        repository::insert_verified_member(tx.conn(), req.event_id, admin_id).await?;
    repository::insert_session(tx.conn(), user_id, &token_hash).await?;
    tx.commit().await?;

    // The token is never recorded here — only that a membership was
    // granted, by whom, and at which run. audit_events is append-only,
    // so a credential written into it could never be taken out.
    audit::write(
        pool,
        "admin",
        Some(admin_id),
        "member.created",
        Some(&user_id.to_string()),
        json!({ "event_id": req.event_id, "member_no": member_no }),
    )
    .await;

    Ok(CreatedMember {
        user_id,
        member_no,
        token,
    })
}
