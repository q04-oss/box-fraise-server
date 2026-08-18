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
    types::{
        AttendanceRecorded, CreateMemberRequest, CreatedMember, MembershipStatus,
        RecordAttendanceRequest,
    },
};
use crate::{
    audit, crypto,
    db::{AdminRlsTransaction, Pool, RlsTransaction},
    error::{AppError, AppResult},
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
    // Signing up is turning up. Without this the account is lapsed
    // from the moment it is made, because bf_member_is_current has
    // nothing to find — 0025 backfills this for members who already
    // existed, and this is the same fact for every member after.
    repository::record_attendance(tx.conn(), user_id, req.event_id, admin_id).await?;
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

/// How long an attendance keeps somebody able to post. Mirrors the
/// interval in `bf_member_is_current` — the database is the authority,
/// and this only exists to tell a member the date.
const MEMBERSHIP_DAYS: i64 = 31;

/// Mark somebody present at a run.
pub async fn record_attendance(
    pool: &Pool,
    admin_id: Uuid,
    req: RecordAttendanceRequest,
) -> AppResult<AttendanceRecorded> {
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let user_id = repository::id_by_member_no(tx.conn(), req.member_no)
        .await?
        .ok_or(AppError::NotFound)?;
    let was_new = repository::record_attendance(tx.conn(), user_id, req.event_id, admin_id).await?;
    let (_, last_seen) = repository::standing(tx.conn(), user_id).await?;
    tx.commit().await?;

    if was_new {
        audit::write(
            pool,
            "admin",
            Some(admin_id),
            "member.attended",
            Some(&user_id.to_string()),
            json!({ "event_id": req.event_id, "member_no": req.member_no }),
        )
        .await;
    }

    Ok(AttendanceRecorded {
        member_no: req.member_no,
        was_new,
        current_until: last_seen.unwrap_or_else(chrono::Utc::now)
            + chrono::Duration::days(MEMBERSHIP_DAYS),
    })
}

/// A member's own standing. Read under their context, so it cannot
/// report on anybody else.
pub async fn standing(pool: &Pool, user_id: Uuid) -> AppResult<MembershipStatus> {
    let mut tx = RlsTransaction::begin(pool, user_id).await?;
    let member_no = repository::member_no(tx.conn(), user_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let (current, last_seen) = repository::standing(tx.conn(), user_id).await?;
    tx.commit().await?;
    Ok(MembershipStatus {
        member_no,
        current,
        last_seen,
        current_until: last_seen.map(|t| t + chrono::Duration::days(MEMBERSHIP_DAYS)),
    })
}
