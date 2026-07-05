use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::FromRow;
use uuid::Uuid;

// ── DB rows ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct SocialVerification {
    pub id: Uuid,
    pub user_id: Uuid,
    pub consulted_by_user_id: Uuid,
    pub salon_id: Option<Uuid>,
    pub consulted_at: DateTime<Utc>,
    pub consultation_notes: Option<String>,
    pub consent_snapshot: Value,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct IdentityCard {
    pub id: Uuid,
    pub user_id: Uuid,
    pub social_verification_id: Uuid,
    pub serial: String,
    pub issued_at: DateTime<Utc>,
    pub issued_by_user_id: Uuid,
    pub salon_id: Option<Uuid>,
    pub design_version: String,
    pub status: String,
    pub replaced_by_card_id: Option<Uuid>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub revoked_reason: Option<String>,
    pub created_at: DateTime<Utc>,
}

// ── Requests ─────────────────────────────────────────────────────────

/// Body of POST /v1/admin/consultations. Records a completed
/// consultation and issues a card in one atomic operation — the two
/// are treated as inseparable at the service layer. If hair_profile
/// is supplied, it's also written in the same transaction.
#[derive(Debug, Deserialize)]
pub struct CompleteConsultationRequest {
    pub user_id: Uuid,
    pub salon_id: Option<Uuid>,
    pub consultation_notes: Option<String>,
    /// What the user consented to at the moment of consultation.
    /// Suggested keys: advertising, social_feed, revenue_share, portrait_capture.
    /// Never require any specific key — the shape is user-driven.
    #[serde(default)]
    pub consent_snapshot: Value,
    #[serde(default = "default_design_version")]
    pub design_version: String,
    /// Optional hair profile captured during the consultation.
    /// Written to hair_profiles in the same admin transaction.
    pub hair_profile: Option<crate::domain::modeling::types::HairProfileInput>,
}

fn default_design_version() -> String {
    "v1".into()
}

/// Response bundles both records — the consultation attestation and
/// the card issued from it.
#[derive(Debug, Serialize)]
pub struct CompleteConsultationResponse {
    pub verification: SocialVerification,
    pub card: IdentityCard,
}

#[derive(Debug, Deserialize)]
pub struct RevokeCardRequest {
    pub reason: String,
}

#[derive(Debug, Deserialize)]
pub struct ReplaceCardRequest {
    /// Optional new design version. Defaults to the current default.
    #[serde(default)]
    pub design_version: Option<String>,
}

// ── Public card lookup ──────────────────────────────────────────────

/// The response shape at GET /v1/cards/{serial}. Contains only what a
/// stranger scanning the card needs to know: is this a real, valid
/// Box Fraise card and when was it issued. Never leaks user_id,
/// consultant_id, salon, or any personal data — a scan tells you the
/// card exists, not who it belongs to.
#[derive(Debug, Serialize)]
pub struct CardLookupResponse {
    pub serial: String,
    pub is_valid: bool,
    pub status: String,
    pub issued_at: DateTime<Utc>,
    pub design_version: String,
}

/// User-facing status shown at GET /v1/me/verification-status.
#[derive(Debug, Serialize)]
pub struct MyVerificationStatus {
    pub tier: u8,
    pub verified: bool,
    pub verification: Option<SocialVerification>,
    pub active_card: Option<IdentityCard>,
}
