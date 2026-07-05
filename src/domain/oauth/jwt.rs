// Minimal ES256 JWT signing + verification. Written directly against
// the p256 crate rather than pulling in a JWT library because this
// keeps the surface small and the intent legible.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use p256::ecdsa::{
    signature::{Signer, Verifier},
    Signature, SigningKey, VerifyingKey,
};
use serde_json::{json, Value};

/// Sign a claims object into a compact-JWS-serialised ES256 JWT.
///
/// Encoding: `base64url(header) . base64url(claims) . base64url(sig)`
/// where `sig` is the 64-byte raw r||s form (JWT ES256 standard),
/// NOT the DER encoding.
pub fn sign(claims: &Value, key: &SigningKey, kid: &str) -> String {
    let header = json!({ "alg": "ES256", "typ": "JWT", "kid": kid });
    let header_b64 = URL_SAFE_NO_PAD.encode(header.to_string().as_bytes());
    let claims_b64 = URL_SAFE_NO_PAD.encode(claims.to_string().as_bytes());
    let signing_input = format!("{header_b64}.{claims_b64}");
    let sig: Signature = key.sign(signing_input.as_bytes());
    let sig_bytes = sig.to_bytes();
    let sig_b64 = URL_SAFE_NO_PAD.encode(&sig_bytes[..]);
    format!("{signing_input}.{sig_b64}")
}

/// Verify + parse a JWT signed by our own key. Returns the claims on
/// success. Any signature failure, malformed part, or non-JSON claim
/// is a hard error.
pub fn verify(jwt: &str, key: &VerifyingKey) -> Option<Value> {
    let parts: Vec<&str> = jwt.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let signing_input = format!("{}.{}", parts[0], parts[1]);
    let sig_bytes = URL_SAFE_NO_PAD.decode(parts[2]).ok()?;
    let sig = Signature::from_slice(&sig_bytes).ok()?;
    key.verify(signing_input.as_bytes(), &sig).ok()?;
    let claims_bytes = URL_SAFE_NO_PAD.decode(parts[1]).ok()?;
    serde_json::from_slice::<Value>(&claims_bytes).ok()
}
