//! The calendar.
//!
//! A member does not have to chase their own schedule. A business
//! publishes it, and what it publishes is what is true.
//!
//! Nothing here writes to `audit_events`. That table is append-only by
//! design, and a permanent record of where somebody was and when — for
//! every member, indefinitely — is the artifact an employer or a court
//! would ask for after a strike month. `messages` refuses an audit
//! entry for the same reason. Shifts are pruned on a TTL by
//! src/maintenance.rs instead, so the calendar is binding in the
//! present without becoming a movement log.

use chrono::{Duration, Utc};
use uuid::Uuid;

use super::{
    repository,
    types::{
        CalendarEntry, EmploymentRecorded, PublishShiftRequest, PublishedShift,
        RecordEmploymentRequest,
    },
};
use crate::{
    db::{AdminRlsTransaction, Pool, RlsTransaction},
    domain::members::repository as members_repository,
    error::{AppError, AppResult},
};

/// How far back the calendar looks. A little, so a shift that started
/// this morning is still on the page while somebody is working it.
const LOOK_BACK_HOURS: i64 = 12;

/// A member's own calendar. Read under their context, so the policy —
/// not this function — is what stops it returning anybody else's.
pub async fn mine(pool: &Pool, user_id: Uuid) -> AppResult<Vec<CalendarEntry>> {
    let mut tx = RlsTransaction::begin(pool, user_id).await?;
    let from = Utc::now() - Duration::hours(LOOK_BACK_HOURS);
    let rows = repository::mine(tx.conn(), user_id, from).await?;
    tx.commit().await?;
    Ok(rows)
}

/// Publish a shift on a business's behalf.
///
/// Admin-only because a business cannot sign in: there is no
/// `business_sessions` table and no business login. Until there is,
/// this is somebody at Box Fraise entering what a business asked for —
/// which is the same shape as every other act on this platform, where
/// an admin does the thing while looking at the person.
///
/// The member is named by number rather than by id, because a number is
/// what a member has and what anybody would read out.
pub async fn publish_shift(
    pool: &Pool,
    admin_id: Uuid,
    req: PublishShiftRequest,
) -> AppResult<PublishedShift> {
    if req.ends_at <= req.starts_at {
        return Err(AppError::bad_request("a shift has to end after it starts"));
    }

    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let user_id = members_repository::id_by_member_no(tx.conn(), req.member_no)
        .await?
        .ok_or(AppError::NotFound)?;
    let id = repository::insert_shift(
        tx.conn(),
        user_id,
        req.business_id,
        req.starts_at,
        req.ends_at,
        admin_id,
    )
    .await?;
    tx.commit().await?;

    Ok(PublishedShift {
        id,
        member_no: req.member_no,
        starts_at: req.starts_at,
        ends_at: req.ends_at,
    })
}

/// Take a published shift away.
///
/// A cancellation, never an edit. The times on a published shift are
/// not changeable — a change is this plus a new shift, and both stay
/// visible to the member, which is the entire difference between this
/// and a schedule the employer can quietly rewrite.
pub async fn cancel_shift(pool: &Pool, admin_id: Uuid, id: Uuid) -> AppResult<()> {
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let done = repository::cancel_shift(tx.conn(), id, admin_id).await?;
    tx.commit().await?;
    if done {
        Ok(())
    } else {
        // Either it does not exist or it was already cancelled.
        Err(AppError::Conflict)
    }
}

/// Record that a member works at a business on the platform.
pub async fn record_employment(
    pool: &Pool,
    admin_id: Uuid,
    req: RecordEmploymentRequest,
) -> AppResult<EmploymentRecorded> {
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let user_id = members_repository::id_by_member_no(tx.conn(), req.member_no)
        .await?
        .ok_or(AppError::NotFound)?;
    let id = repository::insert_employment(tx.conn(), user_id, req.business_id, admin_id).await?;
    tx.commit().await?;

    Ok(EmploymentRecorded {
        // None means it was already recorded, which is not an error.
        id: id.unwrap_or_default(),
        member_no: req.member_no,
        business_id: req.business_id,
    })
}

/// Close it. Employment is a status that ends — unlike attendance,
/// which is a fact about a morning.
pub async fn end_employment(
    pool: &Pool,
    _admin_id: Uuid,
    req: RecordEmploymentRequest,
) -> AppResult<()> {
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let user_id = members_repository::id_by_member_no(tx.conn(), req.member_no)
        .await?
        .ok_or(AppError::NotFound)?;
    let done = repository::end_employment(tx.conn(), user_id, req.business_id).await?;
    tx.commit().await?;
    if done {
        Ok(())
    } else {
        Err(AppError::NotFound)
    }
}
