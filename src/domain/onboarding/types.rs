use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    /// Base64 (url-safe or standard, padded or not) of the SEC1
    /// uncompressed P-256 public key: 0x04 || X(32) || Y(32) = 65 bytes.
    pub public_key: String,
    /// Opaque client-side identifier for the device key. We don't
    /// interpret it; it's there so a future multi-device flow can name
    /// the key.
    pub key_id: String,
}

#[derive(Debug, Serialize)]
pub struct RegisterResponse {
    pub user_id: Uuid,
    pub status: String,
    pub session_token: String,
}

#[derive(Debug, Serialize)]
pub struct ChallengeResponse {
    pub nonce: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct VerifyRequest {
    /// Echoed from the user's QR code. Maps to a challenge row.
    pub nonce: String,
    /// Base64 DER ECDSA(P-256, SHA-256) signature over the nonce bytes.
    pub signature_b64: String,
    /// The event the admin is scanning at — recorded on the user row
    /// for "where did this verification happen" provenance and for the
    /// verified-count metric.
    pub event_id: Uuid,
}

#[derive(Debug, Serialize)]
pub struct VerifyResponse {
    pub user_id: Uuid,
    pub status: String,
    pub verified_at: DateTime<Utc>,
    pub verified_at_event_id: Uuid,
}

#[derive(Debug, Serialize)]
pub struct MeResponse {
    pub id: Uuid,
    pub status: String,
    pub verified_at: Option<DateTime<Utc>>,
    pub verified_at_event_id: Option<Uuid>,
}
