use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const WARNING: &str = "This login API is experimental and not maintained for production use. \
                           Signing keys are regenerated on every restart. Do not integrate.";

#[derive(Debug, Deserialize)]
pub struct TokenRequest {
    /// The `aud` claim to bake into the resulting JWT. Represents
    /// the third-party service the token is intended for.
    pub audience: String,
}

#[derive(Debug, Serialize)]
pub struct TokenResponse {
    pub warning: &'static str,
    pub access_token: String,
    pub token_type: &'static str,
    pub expires_in: i64,
    pub issued_for_audience: String,
}

#[derive(Debug, Serialize)]
pub struct Jwks {
    pub warning: &'static str,
    pub keys: Vec<Jwk>,
}

#[derive(Debug, Serialize)]
pub struct Jwk {
    pub kty: &'static str,
    pub crv: &'static str,
    pub kid: String,
    #[serde(rename = "use")]
    pub use_: &'static str,
    pub alg: &'static str,
    pub x: String,
    pub y: String,
}

#[derive(Debug, Serialize)]
pub struct UserInfo {
    pub warning: &'static str,
    pub iss: &'static str,
    pub sub: Uuid,
    pub tier: u8,
    pub verified: bool,
}
