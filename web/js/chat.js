// The channel that opens after a pairing.
//
// Messages are encrypted here and decrypted here. The server stores
// ciphertext and has no key, so what it can see is that two member
// numbers exchanged something, how much, and when — never the words.
//
// The keypair lives in IndexedDB as a non-extractable CryptoKey: the
// private half cannot be read back out even by this code, only used.
// That is also why there is no history on a second device and no
// recovery. The key lives where the credential lives, which is the
// phone.
//
// Static ECDH, so no forward secrecy — somebody who takes a member's
// private key can read everything that member ever received. Fixing
// that means ratcheting, which is a much larger machine than this
// currently deserves.
(() => {
  'use strict';

  const DB = 'box-fraise-chat';
  const STORE = 'keys';
  const KEY_ID = 'ecdh';
  const TOKEN_KEY = 'bf_member_token';

  const token = () => { try { return localStorage.getItem(TOKEN_KEY); } catch { return null; } };

  const b64 = bytes => btoa(String.fromCharCode(...new Uint8Array(bytes)))
    .replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
  const unb64 = s => Uint8Array.from(
    atob(s.replace(/-/g, '+').replace(/_/g, '/')), c => c.charCodeAt(0));

  function idb() {
    return new Promise((resolve, reject) => {
      const req = indexedDB.open(DB, 1);
      req.onupgradeneeded = () => req.result.createObjectStore(STORE);
      req.onsuccess = () => resolve(req.result);
      req.onerror = () => reject(req.error);
    });
  }

  async function idbGet(key) {
    const db = await idb();
    return new Promise((resolve, reject) => {
      const r = db.transaction(STORE, 'readonly').objectStore(STORE).get(key);
      r.onsuccess = () => resolve(r.result);
      r.onerror = () => reject(r.error);
    });
  }

  async function idbPut(key, value) {
    const db = await idb();
    return new Promise((resolve, reject) => {
      const r = db.transaction(STORE, 'readwrite').objectStore(STORE).put(value, key);
      r.onsuccess = () => resolve();
      r.onerror = () => reject(r.error);
    });
  }

  /// The member's own keypair, made once and kept. Publishing the
  /// public half is what lets somebody else derive a shared secret.
  async function myKeypair() {
    const existing = await idbGet(KEY_ID);
    if (existing) return existing;

    const pair = await crypto.subtle.generateKey(
      { name: 'ECDH', namedCurve: 'P-256' },
      false,                       // not extractable: usable, never readable
      ['deriveKey', 'deriveBits'],
    );
    await idbPut(KEY_ID, pair);

    const raw = await crypto.subtle.exportKey('raw', pair.publicKey);
    const r = await fetch('/v1/members/me/key', {
      method: 'PUT',
      headers: {
        'Authorization': 'Bearer ' + token(),
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({ public_key: b64(raw) }),
    });
    if (!r.ok) throw new Error('could not publish the key');
    return pair;
  }

  /// One AES-GCM key per conversation, derived from both halves. Cached
  /// per page load; the derivation is cheap but the round trip for the
  /// peer's key is not.
  const secrets = new Map();

  async function sharedKey(pairingId, peerId) {
    if (secrets.has(pairingId)) return secrets.get(pairingId);

    const pair = await myKeypair();
    const r = await fetch('/v1/pairings/' + peerId + '/key', {
      headers: { 'Authorization': 'Bearer ' + token() },
    });
    if (r.status === 404) throw new Error('they have not opened this on a device yet');
    if (!r.ok) throw new Error('could not fetch their key');
    const { public_key } = await r.json();

    const peer = await crypto.subtle.importKey(
      'raw', unb64(public_key),
      { name: 'ECDH', namedCurve: 'P-256' },
      false, [],
    );
    const key = await crypto.subtle.deriveKey(
      { name: 'ECDH', public: peer },
      pair.privateKey,
      { name: 'AES-GCM', length: 256 },
      false,
      ['encrypt', 'decrypt'],
    );
    secrets.set(pairingId, key);
    return key;
  }

  window.bfChat = {
    /// Make and publish this device's key. Called before a channel is
    /// used, so a member who never chats never gets one.
    ready: myKeypair,

    async send(pairingId, peerId, text) {
      const key = await sharedKey(pairingId, peerId);
      const iv = crypto.getRandomValues(new Uint8Array(12));
      const ct = await crypto.subtle.encrypt(
        { name: 'AES-GCM', iv }, key, new TextEncoder().encode(text));

      const r = await fetch('/v1/pairings/' + pairingId + '/messages', {
        method: 'POST',
        headers: {
          'Authorization': 'Bearer ' + token(),
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({ ciphertext: b64(ct), iv: b64(iv) }),
      });
      if (!r.ok) throw new Error('it did not send');
      return r.json();
    },

    async read(pairingId, peerId) {
      const key = await sharedKey(pairingId, peerId);
      const r = await fetch('/v1/pairings/' + pairingId + '/messages', {
        headers: { 'Authorization': 'Bearer ' + token() },
      });
      if (!r.ok) throw new Error('could not read the channel');
      const rows = await r.json();

      return Promise.all(rows.map(async m => {
        let text;
        try {
          const plain = await crypto.subtle.decrypt(
            { name: 'AES-GCM', iv: unb64(m.iv) }, key, unb64(m.ciphertext));
          text = new TextDecoder().decode(plain);
        } catch {
          // Undecryptable rather than lost: usually a message from
          // before somebody replaced their key. Say so instead of
          // showing nothing.
          text = null;
        }
        return { id: m.id, sender_id: m.sender_id, at: m.created_at, text };
      }));
    },
  };
})();
