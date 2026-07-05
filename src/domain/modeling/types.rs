use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

// ── Hair profile ────────────────────────────────────────────────────

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct HairProfile {
    pub user_id: Uuid,
    pub hair_length: Option<String>,
    pub hair_texture: Option<String>,
    pub hair_type: Option<String>,
    pub hair_thickness: Option<String>,
    pub natural_color: Option<String>,
    pub current_color: Option<String>,
    pub chemically_treated: bool,
    pub willing_services: Option<Vec<String>>,
    pub willing_to_model: bool,
    pub is_hair_student: bool,
    pub hair_notes: Option<String>,
    pub updated_at: DateTime<Utc>,
}

/// What a consultant records during a consultation. All fields are
/// optional so users can decline to disclose any specific attribute.
/// The service layer never allows a user to submit this themselves —
/// only via the consultation completion path (consultant enters).
#[derive(Debug, Clone, Deserialize)]
pub struct HairProfileInput {
    pub hair_length: Option<String>,
    pub hair_texture: Option<String>,
    pub hair_type: Option<String>,
    pub hair_thickness: Option<String>,
    pub natural_color: Option<String>,
    pub current_color: Option<String>,
    #[serde(default)]
    pub chemically_treated: bool,
    pub willing_services: Option<Vec<String>>,
    #[serde(default)]
    pub willing_to_model: bool,
    #[serde(default)]
    pub is_hair_student: bool,
    pub hair_notes: Option<String>,
}

/// User-side update. Deliberately limited: only the willingness
/// toggle. Everything else about hair requires a consultant to change,
/// which matches the "consultant asks, user doesn't type" principle.
#[derive(Debug, Deserialize)]
pub struct UpdateOwnHairProfileRequest {
    pub willing_to_model: bool,
}

// ── Model request ───────────────────────────────────────────────────

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct ModelRequest {
    pub id: Uuid,
    pub student_user_id: Uuid,
    pub service: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub location: String,
    pub location_lat: Option<f64>,
    pub location_lng: Option<f64>,
    pub filter_length: Vec<String>,
    pub filter_texture: Vec<String>,
    pub filter_type: Vec<String>,
    pub filter_color: Vec<String>,
    pub additional_notes: Option<String>,
    pub status: String,
    pub filled_by_user_id: Option<Uuid>,
    pub filled_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateModelRequestRequest {
    pub service: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub location: String,
    pub location_lat: Option<f64>,
    pub location_lng: Option<f64>,
    #[serde(default)]
    pub filter_length: Vec<String>,
    #[serde(default)]
    pub filter_texture: Vec<String>,
    #[serde(default)]
    pub filter_type: Vec<String>,
    #[serde(default)]
    pub filter_color: Vec<String>,
    pub additional_notes: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateModelRequestResponse {
    pub request: ModelRequest,
    pub invitations_sent: i64,
}

// ── Invitations ─────────────────────────────────────────────────────

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct ModelInvitation {
    pub id: Uuid,
    pub model_request_id: Uuid,
    pub potential_model_user_id: Uuid,
    pub invited_at: DateTime<Utc>,
    pub responded_at: Option<DateTime<Utc>>,
    pub response: Option<String>,
    pub schedule_item_id: Option<Uuid>,
}

/// What the user actually sees when browsing incoming invitations:
/// the invitation joined with the request so they can decide with the
/// time/place/details in front of them. Student's user_id included
/// for handles / future messaging — not their name.
#[derive(Debug, Serialize)]
pub struct InvitationWithContext {
    pub invitation: ModelInvitation,
    pub request: ModelRequest,
}
