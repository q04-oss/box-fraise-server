//! The channel that opens after a pairing.
//!
//! Everything here handles ciphertext. The key is derived in the two
//! browsers from an ECDH exchange and never reaches this process, so
//! nothing in this file — and nothing an operator can run against the
//! database — turns a message back into words.
//!
//! Who may talk to whom is decided by RLS in 0026, not here. These
//! functions open a transaction under the caller's own context and let
//! the policies answer.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use uuid::Uuid;

use super::{repository, types::*};
use crate::{
    db::{Pool, RlsTransaction},
    error::{AppError, AppResult},
};

/// Raw P-256 SEC1 uncompressed: 0x04 || X(32) || Y(32).
const KEY_LEN: usize = 65;
/// The GCM nonce.
const IV_LEN: usize = 12;
/// Mirrors `messages_size`. Roughly six kilobytes of text.
const MAX_CIPHERTEXT: usize = 8192;

fn decode(field: &str, value: &str) -> AppResult<Vec<u8>> {
    URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| AppError::bad_request(format!("{field} is not base64url")))
}

pub async fn publish_key(pool: &Pool, user_id: Uuid, req: PublishKeyRequest) -> AppResult<()> {
    let key = decode("public_key", &req.public_key)?;
    if key.len() != KEY_LEN || key[0] != 0x04 {
        return Err(AppError::bad_request(
            "a public key is 65 bytes of uncompressed P-256",
        ));
    }
    let mut tx = RlsTransaction::begin(pool, user_id).await?;
    repository::upsert_key(tx.conn(), user_id, &key).await?;
    tx.commit().await?;
    Ok(())
}

/// The other person's key, if there is an open channel. NotFound
/// covers both "no such channel" and "they have not published one" —
/// the caller cannot tell which, and does not need to.
pub async fn peer_key(pool: &Pool, user_id: Uuid, peer_id: Uuid) -> AppResult<PeerKey> {
    let mut tx = RlsTransaction::begin(pool, user_id).await?;
    let key = repository::peer_key(tx.conn(), peer_id).await?;
    tx.commit().await?;
    Ok(PeerKey {
        public_key: URL_SAFE_NO_PAD.encode(key.ok_or(AppError::NotFound)?),
    })
}

pub async fn send(
    pool: &Pool,
    user_id: Uuid,
    pairing_id: Uuid,
    req: SendMessageRequest,
) -> AppResult<Message> {
    let ciphertext = decode("ciphertext", &req.ciphertext)?;
    let iv = decode("iv", &req.iv)?;
    if iv.len() != IV_LEN {
        return Err(AppError::bad_request("iv must be 12 bytes"));
    }
    if ciphertext.len() > MAX_CIPHERTEXT || ciphertext.len() < 17 {
        return Err(AppError::bad_request("message is empty or too long"));
    }

    let mut tx = RlsTransaction::begin(pool, user_id).await?;
    // If the channel is not open to this member the INSERT policy
    // refuses, which surfaces as a plain refusal rather than a hint
    // about whether the pairing exists.
    let (id, created_at) =
        repository::insert_message(tx.conn(), pairing_id, user_id, &ciphertext, &iv)
            .await
            .map_err(|_| AppError::Forbidden)?;
    tx.commit().await?;

    // Deliberately no audit entry. A message is the one thing on this
    // platform nobody is supposed to be able to account for, and
    // audit_events is append-only — a record of who wrote to whom, and
    // when, could never be taken back out.
    Ok(Message {
        id,
        sender_id: user_id,
        ciphertext: req.ciphertext,
        iv: req.iv,
        created_at,
    })
}

pub async fn list(pool: &Pool, user_id: Uuid, pairing_id: Uuid) -> AppResult<Vec<Message>> {
    let mut tx = RlsTransaction::begin(pool, user_id).await?;
    let rows = repository::list_messages(tx.conn(), pairing_id).await?;
    tx.commit().await?;
    Ok(rows
        .into_iter()
        .map(|(id, sender_id, ciphertext, iv, created_at)| Message {
            id,
            sender_id,
            ciphertext: URL_SAFE_NO_PAD.encode(ciphertext),
            iv: URL_SAFE_NO_PAD.encode(iv),
            created_at,
        })
        .collect())
}
