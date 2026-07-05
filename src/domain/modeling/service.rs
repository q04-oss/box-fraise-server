use serde_json::json;
use uuid::Uuid;

use crate::{
    audit,
    db::{AdminRlsTransaction, Pool, RlsTransaction},
    domain::modeling::{repository, types::*},
    error::{AppError, AppResult},
};

// ── Hair profile (user side) ────────────────────────────────────────

pub async fn get_own_hair_profile(pool: &Pool, user_id: Uuid) -> AppResult<Option<HairProfile>> {
    let mut tx = RlsTransaction::begin(pool, user_id).await?;
    let profile = repository::get_own_hair_profile(tx.conn(), user_id).await?;
    tx.commit().await?;
    Ok(profile)
}

pub async fn update_own_willing_to_model(
    pool: &Pool,
    user_id: Uuid,
    req: UpdateOwnHairProfileRequest,
) -> AppResult<HairProfile> {
    let mut tx = RlsTransaction::begin(pool, user_id).await?;
    let updated = repository::update_willing_to_model(tx.conn(), user_id, req.willing_to_model)
        .await?
        .ok_or_else(|| {
            AppError::bad_request(
                "no hair profile on file — a consultation must record hair information first",
            )
        })?;
    tx.commit().await?;
    Ok(updated)
}

// ── Model requests (student side) ───────────────────────────────────

pub async fn create_model_request(
    pool: &Pool,
    student_user_id: Uuid,
    req: CreateModelRequestRequest,
) -> AppResult<CreateModelRequestResponse> {
    // Basic validation.
    let service = req.service.trim();
    if service.is_empty() {
        return Err(AppError::bad_request("service description required"));
    }
    if req.location.trim().is_empty() {
        return Err(AppError::bad_request("location required"));
    }
    if req.ends_at <= req.starts_at {
        return Err(AppError::bad_request("ends_at must be after starts_at"));
    }

    // Verify the caller is a hair student — the acting user must have
    // hair_profiles.is_hair_student = true.
    let mut check_tx = RlsTransaction::begin(pool, student_user_id).await?;
    let profile = repository::get_own_hair_profile(check_tx.conn(), student_user_id).await?;
    check_tx.commit().await?;
    let is_student = profile.as_ref().is_some_and(|p| p.is_hair_student);
    if !is_student {
        return Err(AppError::Forbidden);
    }

    // Insert request + fan out invitations under admin context.
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let request = repository::insert_request(
        tx.conn(),
        student_user_id,
        service,
        req.starts_at,
        req.ends_at,
        req.location.trim(),
        req.location_lat,
        req.location_lng,
        &req.filter_length,
        &req.filter_texture,
        &req.filter_type,
        &req.filter_color,
        req.additional_notes.as_deref(),
    )
    .await?;
    let invitations_sent = repository::fan_out_invitations(
        tx.conn(),
        request.id,
        student_user_id,
        &req.filter_length,
        &req.filter_texture,
        &req.filter_type,
        &req.filter_color,
    )
    .await?;
    tx.commit().await?;

    audit::write(
        pool,
        "user",
        Some(student_user_id),
        "model_request.create",
        Some(&request.id.to_string()),
        json!({
            "invitations_sent": invitations_sent,
            "starts_at": request.starts_at,
        }),
    )
    .await;

    Ok(CreateModelRequestResponse {
        request,
        invitations_sent,
    })
}

pub async fn list_own_requests(pool: &Pool, student_user_id: Uuid) -> AppResult<Vec<ModelRequest>> {
    let mut tx = RlsTransaction::begin(pool, student_user_id).await?;
    let requests = repository::list_requests_for_student(tx.conn(), student_user_id).await?;
    tx.commit().await?;
    Ok(requests)
}

pub async fn cancel_own_request(
    pool: &Pool,
    student_user_id: Uuid,
    request_id: Uuid,
) -> AppResult<ModelRequest> {
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let cancelled = repository::cancel_request(tx.conn(), request_id, student_user_id).await?;
    tx.commit().await?;
    let request = cancelled.ok_or(AppError::NotFound)?;

    audit::write(
        pool,
        "user",
        Some(student_user_id),
        "model_request.cancel",
        Some(&request.id.to_string()),
        json!({}),
    )
    .await;
    Ok(request)
}

// ── Invitations (model side) ────────────────────────────────────────

pub async fn list_own_invitations(
    pool: &Pool,
    user_id: Uuid,
) -> AppResult<Vec<InvitationWithContext>> {
    // Model invitations belong to the model (RLS enforces this on the
    // invitations table). Fetching the associated model_requests
    // requires elevated context: an invitee needs to see the request
    // details (time, location, hair criteria) to decide whether to
    // accept, but under user-scoped RLS they can't read requests they
    // didn't originate. We run the entire fetch under an
    // AdminRlsTransaction and re-enforce ownership in the query
    // (WHERE potential_model_user_id = $1) — the application code is
    // the audit boundary for this narrow path.
    let mut tx = AdminRlsTransaction::begin(pool).await?;
    let rows = repository::list_invitations_for_model(tx.conn(), user_id).await?;
    tx.commit().await?;
    Ok(rows
        .into_iter()
        .map(|(invitation, request)| InvitationWithContext {
            invitation,
            request,
        })
        .collect())
}

pub async fn respond_to_invitation(
    pool: &Pool,
    user_id: Uuid,
    invitation_id: Uuid,
    accept: bool,
) -> AppResult<ModelInvitation> {
    let response_str = if accept { "accepted" } else { "declined" };

    // Under admin context so we can both mark the invitation AND flip
    // the request to filled if accepted.
    let mut tx = AdminRlsTransaction::begin(pool).await?;

    // Confirm the invitation exists, is unresponded, and belongs to this user.
    let invitation = repository::get_invitation(tx.conn(), invitation_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if invitation.potential_model_user_id != user_id {
        return Err(AppError::Forbidden);
    }
    if invitation.response.is_some() {
        return Err(AppError::Conflict);
    }

    // Confirm the request is still open (someone may have accepted first).
    let request = repository::get_request(tx.conn(), invitation.model_request_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if request.status != "open" {
        return Err(AppError::Conflict);
    }

    let updated =
        repository::set_invitation_response(tx.conn(), invitation_id, user_id, response_str)
            .await?
            .ok_or(AppError::Conflict)?;

    if accept {
        // Mark the request filled. If a race is lost (request no longer
        // open), roll back so the invitation stays unresponded.
        if repository::mark_filled(tx.conn(), invitation.model_request_id, user_id)
            .await?
            .is_none()
        {
            tx.rollback().await?;
            return Err(AppError::Conflict);
        }
    }

    tx.commit().await?;

    audit::write(
        pool,
        "user",
        Some(user_id),
        if accept {
            "model_invitation.accept"
        } else {
            "model_invitation.decline"
        },
        Some(&invitation_id.to_string()),
        json!({
            "model_request_id": invitation.model_request_id,
        }),
    )
    .await;

    Ok(updated)
}
