use std::sync::OnceLock;

use p256::ecdsa::{SigningKey, VerifyingKey};
use sha2::{Digest, Sha256};

// Ephemeral, per-process signing key. Generated on first access.
// Regenerated on every restart — a hard, intentional signal that this
// API is not stable and should not be integrated against.
static SIGNING_KEY: OnceLock<SigningKey> = OnceLock::new();

pub fn signing_key() -> &'static SigningKey {
    SIGNING_KEY.get_or_init(|| SigningKey::random(&mut rand::rngs::OsRng))
}

pub fn verifying_key() -> VerifyingKey {
    *signing_key().verifying_key()
}

/// Key id used in the JWT `kid` claim and the JWKS entry. Deterministic
/// from the public key material, so it's stable for the process
/// lifetime and changes on restart alongside the key itself.
pub fn key_id() -> String {
    let vk = verifying_key();
    let ep = vk.to_encoded_point(false);
    let mut hasher = Sha256::new();
    hasher.update(ep.as_bytes());
    let digest = hasher.finalize();
    hex(&digest[..8])
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}
