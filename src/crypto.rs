// Cryptographic primitives:
//   - P-256 ECDSA verification (Apple Secure Enclave compatible)
//   - Session token gen + hashing
//   - Nonce generation
//   - Argon2id for admin passwords

use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE, URL_SAFE_NO_PAD},
    Engine as _,
};
use p256::ecdsa::{signature::Verifier, Signature, VerifyingKey};
use rand::RngCore;
use sha2::{Digest, Sha256};

use crate::error::AppError;

/// Verify a P-256 ECDSA signature over the `nonce` UTF-8 bytes.
///
/// Mirrors what `SecKeyCreateSignature(.., .ecdsaSignatureMessageX962SHA256, ..)`
/// produces on iOS:
///   - SEC1 uncompressed public key (0x04 || X || Y), 65 bytes
///   - DER-encoded (r, s) signature
///   - SHA-256 prehash of the message (built into ecdsa::Verifier::verify
///     when used with the standard signing/verifying key types)
///   - Possibly *not* low-S normalised (Apple may emit high-S).
///
/// We normalise S before verifying so a valid high-S signature out of
/// the Secure Enclave is accepted. The Signature equivalence class
/// `(r, s) ~ (r, -s mod n)` means both forms represent the same valid
/// signature, and most strict verifiers reject high-S by default.
pub fn verify_p256_signature(
    public_key_sec1: &[u8],
    nonce: &str,
    signature_der: &[u8],
) -> Result<(), AppError> {
    let pk = p256::PublicKey::from_sec1_bytes(public_key_sec1)
        .map_err(|_| AppError::InvalidSignature)?;
    let verifying_key = VerifyingKey::from(&pk);
    let sig = Signature::from_der(signature_der).map_err(|_| AppError::InvalidSignature)?;
    let sig_to_verify = sig.normalize_s().unwrap_or(sig);
    verifying_key
        .verify(nonce.as_bytes(), &sig_to_verify)
        .map_err(|_| AppError::InvalidSignature)
}

/// Generate a new opaque bearer token. Returns (raw_token, sha256_hex).
/// Only the hash is persisted; the raw token is returned to the client
/// once.
pub fn new_session_token() -> (String, String) {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let token = URL_SAFE_NO_PAD.encode(bytes);
    let hash = sha256_hex(token.as_bytes());
    (token, hash)
}

pub fn sha256_hex(input: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(input);
    let out = h.finalize();
    hex_encode(&out)
}

/// A 32-byte random nonce, base64url-no-pad.
pub fn new_nonce() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Tolerant base64 decoder: tries url-safe-no-pad, url-safe, then standard.
/// Useful because iOS clients sometimes send padded, sometimes not.
pub fn b64_decode(input: &str) -> Result<Vec<u8>, AppError> {
    URL_SAFE_NO_PAD
        .decode(input)
        .or_else(|_| URL_SAFE.decode(input))
        .or_else(|_| STANDARD.decode(input))
        .map_err(|_| AppError::bad_request("invalid base64"))
}

pub fn argon2_hash(password: &str) -> anyhow::Result<String> {
    use argon2::{
        password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
        Argon2,
    };
    let salt = SaltString::generate(&mut OsRng);
    let argon = Argon2::default();
    let hash = argon
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("argon2 hash: {e}"))?
        .to_string();
    Ok(hash)
}

pub fn argon2_verify(password: &str, hash: &str) -> bool {
    use argon2::{password_hash::PasswordVerifier, Argon2, PasswordHash};
    let Ok(parsed) = PasswordHash::new(hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}
