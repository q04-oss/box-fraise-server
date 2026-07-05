use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

use super::types::{HairProfile, HairProfileInput, ModelInvitation, ModelRequest};

// ── Hair profile ────────────────────────────────────────────────────

pub async fn upsert_hair_profile(
    conn: &mut PgConnection,
    user_id: Uuid,
    input: &HairProfileInput,
) -> sqlx::Result<HairProfile> {
    let row = sqlx::query_as::<_, HairProfile>(
        "INSERT INTO hair_profiles
            (user_id, hair_length, hair_texture, hair_type, hair_thickness,
             natural_color, current_color, chemically_treated, willing_services,
             willing_to_model, is_hair_student, hair_notes)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
         ON CONFLICT (user_id) DO UPDATE
            SET hair_length         = EXCLUDED.hair_length,
                hair_texture        = EXCLUDED.hair_texture,
                hair_type           = EXCLUDED.hair_type,
                hair_thickness      = EXCLUDED.hair_thickness,
                natural_color       = EXCLUDED.natural_color,
                current_color       = EXCLUDED.current_color,
                chemically_treated  = EXCLUDED.chemically_treated,
                willing_services    = EXCLUDED.willing_services,
                willing_to_model    = EXCLUDED.willing_to_model,
                is_hair_student     = EXCLUDED.is_hair_student,
                hair_notes          = EXCLUDED.hair_notes,
                updated_at          = now()
         RETURNING *",
    )
    .bind(user_id)
    .bind(&input.hair_length)
    .bind(&input.hair_texture)
    .bind(&input.hair_type)
    .bind(&input.hair_thickness)
    .bind(&input.natural_color)
    .bind(&input.current_color)
    .bind(input.chemically_treated)
    .bind(&input.willing_services)
    .bind(input.willing_to_model)
    .bind(input.is_hair_student)
    .bind(&input.hair_notes)
    .fetch_one(conn)
    .await?;
    Ok(row)
}

pub async fn get_own_hair_profile(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<Option<HairProfile>> {
    let row = sqlx::query_as::<_, HairProfile>("SELECT * FROM hair_profiles WHERE user_id = $1")
        .bind(user_id)
        .fetch_optional(conn)
        .await?;
    Ok(row)
}

pub async fn update_willing_to_model(
    conn: &mut PgConnection,
    user_id: Uuid,
    willing_to_model: bool,
) -> sqlx::Result<Option<HairProfile>> {
    let row = sqlx::query_as::<_, HairProfile>(
        "UPDATE hair_profiles
            SET willing_to_model = $1, updated_at = now()
          WHERE user_id = $2
          RETURNING *",
    )
    .bind(willing_to_model)
    .bind(user_id)
    .fetch_optional(conn)
    .await?;
    Ok(row)
}

// ── Model requests ──────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub async fn insert_request(
    conn: &mut PgConnection,
    student_user_id: Uuid,
    service: &str,
    starts_at: DateTime<Utc>,
    ends_at: DateTime<Utc>,
    location: &str,
    location_lat: Option<f64>,
    location_lng: Option<f64>,
    filter_length: &[String],
    filter_texture: &[String],
    filter_type: &[String],
    filter_color: &[String],
    additional_notes: Option<&str>,
) -> sqlx::Result<ModelRequest> {
    let row = sqlx::query_as::<_, ModelRequest>(
        "INSERT INTO model_requests
            (student_user_id, service, starts_at, ends_at, location,
             location_lat, location_lng, filter_length, filter_texture,
             filter_type, filter_color, additional_notes)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
         RETURNING *",
    )
    .bind(student_user_id)
    .bind(service)
    .bind(starts_at)
    .bind(ends_at)
    .bind(location)
    .bind(location_lat)
    .bind(location_lng)
    .bind(filter_length)
    .bind(filter_texture)
    .bind(filter_type)
    .bind(filter_color)
    .bind(additional_notes)
    .fetch_one(conn)
    .await?;
    Ok(row)
}

/// Fan out invitations to every willing-to-model user whose hair
/// matches the request's filters. Empty filter arrays mean "no
/// filter on that dimension." Returns the number of invitations
/// created.
pub async fn fan_out_invitations(
    conn: &mut PgConnection,
    request_id: Uuid,
    student_user_id: Uuid,
    filter_length: &[String],
    filter_texture: &[String],
    filter_type: &[String],
    filter_color: &[String],
) -> sqlx::Result<i64> {
    // Insert invitations for every matching user (excluding the student
    // themselves — they don't get invited to model for their own
    // request). ON CONFLICT DO NOTHING makes this idempotent.
    let count: (i64,) = sqlx::query_as(
        "WITH inserted AS (
             INSERT INTO model_invitations (model_request_id, potential_model_user_id)
             SELECT $1, hp.user_id
               FROM hair_profiles hp
              WHERE hp.willing_to_model = true
                AND hp.user_id <> $2
                AND (array_length($3::text[], 1) IS NULL OR hp.hair_length   = ANY($3))
                AND (array_length($4::text[], 1) IS NULL OR hp.hair_texture  = ANY($4))
                AND (array_length($5::text[], 1) IS NULL OR hp.hair_type     = ANY($5))
                AND (array_length($6::text[], 1) IS NULL OR hp.natural_color = ANY($6))
             ON CONFLICT (model_request_id, potential_model_user_id) DO NOTHING
             RETURNING id
         ) SELECT COUNT(*)::bigint FROM inserted",
    )
    .bind(request_id)
    .bind(student_user_id)
    .bind(filter_length)
    .bind(filter_texture)
    .bind(filter_type)
    .bind(filter_color)
    .fetch_one(conn)
    .await?;
    Ok(count.0)
}

pub async fn get_request(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<Option<ModelRequest>> {
    let row = sqlx::query_as::<_, ModelRequest>("SELECT * FROM model_requests WHERE id = $1")
        .bind(id)
        .fetch_optional(conn)
        .await?;
    Ok(row)
}

pub async fn list_requests_for_student(
    conn: &mut PgConnection,
    student_user_id: Uuid,
) -> sqlx::Result<Vec<ModelRequest>> {
    let rows = sqlx::query_as::<_, ModelRequest>(
        "SELECT * FROM model_requests
          WHERE student_user_id = $1
          ORDER BY created_at DESC",
    )
    .bind(student_user_id)
    .fetch_all(conn)
    .await?;
    Ok(rows)
}

pub async fn cancel_request(
    conn: &mut PgConnection,
    id: Uuid,
    student_user_id: Uuid,
) -> sqlx::Result<Option<ModelRequest>> {
    let row = sqlx::query_as::<_, ModelRequest>(
        "UPDATE model_requests
            SET status = 'cancelled'
          WHERE id = $1 AND student_user_id = $2 AND status = 'open'
          RETURNING *",
    )
    .bind(id)
    .bind(student_user_id)
    .fetch_optional(conn)
    .await?;
    Ok(row)
}

pub async fn mark_filled(
    conn: &mut PgConnection,
    id: Uuid,
    filled_by_user_id: Uuid,
) -> sqlx::Result<Option<ModelRequest>> {
    let row = sqlx::query_as::<_, ModelRequest>(
        "UPDATE model_requests
            SET status            = 'filled',
                filled_by_user_id = $1,
                filled_at         = now()
          WHERE id = $2 AND status = 'open'
          RETURNING *",
    )
    .bind(filled_by_user_id)
    .bind(id)
    .fetch_optional(conn)
    .await?;
    Ok(row)
}

// ── Model invitations ───────────────────────────────────────────────

pub async fn get_invitation(
    conn: &mut PgConnection,
    id: Uuid,
) -> sqlx::Result<Option<ModelInvitation>> {
    let row = sqlx::query_as::<_, ModelInvitation>("SELECT * FROM model_invitations WHERE id = $1")
        .bind(id)
        .fetch_optional(conn)
        .await?;
    Ok(row)
}

pub async fn list_invitations_for_model(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<Vec<(ModelInvitation, ModelRequest)>> {
    // sqlx has no clean "fetch tuple of two FromRow types" path when
    // both share a column name (both tables have `id`, `created_at`,
    // etc). Two queries + client-side join is simpler and keeps the
    // types crisp.
    let invitations = sqlx::query_as::<_, ModelInvitation>(
        "SELECT * FROM model_invitations
          WHERE potential_model_user_id = $1
          ORDER BY invited_at DESC",
    )
    .bind(user_id)
    .fetch_all(&mut *conn)
    .await?;

    if invitations.is_empty() {
        return Ok(Vec::new());
    }

    let request_ids: Vec<Uuid> = invitations.iter().map(|i| i.model_request_id).collect();
    let requests =
        sqlx::query_as::<_, ModelRequest>("SELECT * FROM model_requests WHERE id = ANY($1)")
            .bind(&request_ids)
            .fetch_all(&mut *conn)
            .await?;

    let by_id: std::collections::HashMap<Uuid, ModelRequest> =
        requests.into_iter().map(|r| (r.id, r)).collect();

    let mut out = Vec::with_capacity(invitations.len());
    for inv in invitations {
        if let Some(req) = by_id.get(&inv.model_request_id).cloned() {
            out.push((inv, req));
        }
    }
    Ok(out)
}

pub async fn set_invitation_response(
    conn: &mut PgConnection,
    id: Uuid,
    user_id: Uuid,
    response: &str,
) -> sqlx::Result<Option<ModelInvitation>> {
    let row = sqlx::query_as::<_, ModelInvitation>(
        "UPDATE model_invitations
            SET response     = $1,
                responded_at = now()
          WHERE id                       = $2
            AND potential_model_user_id  = $3
            AND response IS NULL
          RETURNING *",
    )
    .bind(response)
    .bind(id)
    .bind(user_id)
    .fetch_optional(conn)
    .await?;
    Ok(row)
}
