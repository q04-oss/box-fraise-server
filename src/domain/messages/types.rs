use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Deserialize)]
pub struct PublishKeyRequest {
    /// Raw P-256 ECDH public key, SEC1 uncompressed, base64url. The
    /// private half never leaves the browser that made it.
    pub public_key: String,
}

#[derive(Serialize)]
pub struct PeerKey {
    pub public_key: String,
}

#[derive(Deserialize)]
pub struct SendMessageRequest {
    /// AES-GCM ciphertext, base64url. The server never sees plaintext
    /// and has no key to make any.
    pub ciphertext: String,
    /// The 12-byte GCM nonce, base64url.
    pub iv: String,
}

#[derive(Serialize)]
pub struct Message {
    pub id: Uuid,
    pub sender_id: Uuid,
    pub ciphertext: String,
    pub iv: String,
    pub created_at: DateTime<Utc>,
}
