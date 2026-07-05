// Box Fraise browser identity library.
//
// Handles the same crypto contract as the iOS app spec:
//   - Non-extractable P-256 keypair generated via WebCrypto
//   - Private key persisted in IndexedDB (never leaves the browser
//     profile; a keypair with `extractable=false` cannot be exported
//     by any script, only used to sign)
//   - Public key sent as SEC1 uncompressed (65 bytes, 0x04 || X || Y)
//   - Signatures over the challenge nonce, DER-encoded, SHA-256 prehash
//     — WebCrypto emits IEEE P1363 (raw r||s), we convert to DER before
//     sending so the server (which expects the iOS/DER format) accepts it
//
// The bearer session_token lives in localStorage. It's paired with the
// device key; both together identify a Box Fraise user.

const IDB_NAME     = 'boxfraise';
const IDB_STORE    = 'keys';
const IDB_VERSION  = 1;

const LS_TOKEN     = 'bf_session_token';
const LS_USER_ID   = 'bf_user_id';
const LS_KEY_ID    = 'bf_key_id';

// ── IndexedDB (CryptoKey storage) ─────────────────────────────────────

function openDB() {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(IDB_NAME, IDB_VERSION);
    req.onupgradeneeded = () => req.result.createObjectStore(IDB_STORE);
    req.onsuccess = () => resolve(req.result);
    req.onerror   = () => reject(req.error);
  });
}

async function idbPut(key, value) {
  const db = await openDB();
  return new Promise((resolve, reject) => {
    const tx = db.transaction(IDB_STORE, 'readwrite');
    tx.objectStore(IDB_STORE).put(value, key);
    tx.oncomplete = () => resolve();
    tx.onerror    = () => reject(tx.error);
  });
}

async function idbGet(key) {
  const db = await openDB();
  return new Promise((resolve, reject) => {
    const tx = db.transaction(IDB_STORE, 'readonly');
    const req = tx.objectStore(IDB_STORE).get(key);
    req.onsuccess = () => resolve(req.result);
    req.onerror   = () => reject(req.error);
  });
}

// ── Keypair lifecycle ────────────────────────────────────────────────

async function generateKeypair() {
  // extractable=false → the private key can never be read back out
  // of the browser. Only sign() operations are permitted. This is the
  // strongest guarantee WebCrypto offers.
  return await crypto.subtle.generateKey(
    { name: 'ECDSA', namedCurve: 'P-256' },
    false,
    ['sign']
  );
}

async function loadKeypair() {
  const privateKey = await idbGet('privateKey');
  const publicKey  = await idbGet('publicKey');
  if (!privateKey || !publicKey) return null;
  return { privateKey, publicKey };
}

async function saveKeypair(keypair) {
  await idbPut('privateKey', keypair.privateKey);
  await idbPut('publicKey',  keypair.publicKey);
}

// SEC1 uncompressed = "raw" export in WebCrypto — 65 bytes: 0x04 || X || Y.
// The server calls this "public_key" in the register body.
async function exportPublicKeyRaw(publicKey) {
  const raw = await crypto.subtle.exportKey('raw', publicKey);
  return new Uint8Array(raw);
}

// ── Signature format conversion ──────────────────────────────────────
//
// WebCrypto emits IEEE P1363: r (32 bytes) || s (32 bytes).
// Backend expects DER (X9.62):
//   SEQUENCE { INTEGER r, INTEGER s }
// with the standard DER quirk that integers with a set high bit get a
// leading zero to keep them positive.

function rawSignatureToDer(raw) {
  if (raw.length !== 64) throw new Error('unexpected raw sig length: ' + raw.length);
  const rDer = encodeAsn1Integer(raw.subarray(0, 32));
  const sDer = encodeAsn1Integer(raw.subarray(32, 64));
  const seqLen = rDer.length + sDer.length;
  // Assume seqLen < 128 (always true for P-256 sigs) so length is one byte.
  const out = new Uint8Array(2 + seqLen);
  out[0] = 0x30;        // SEQUENCE
  out[1] = seqLen;
  out.set(rDer, 2);
  out.set(sDer, 2 + rDer.length);
  return out;
}

function encodeAsn1Integer(bytes) {
  // Strip leading zeros while keeping at least one byte.
  let start = 0;
  while (start < bytes.length - 1 && bytes[start] === 0) start++;
  const trimmed = bytes.subarray(start);
  // If high bit set, prepend a zero to keep the value positive.
  const needsPad = (trimmed[0] & 0x80) !== 0;
  const contentLen = trimmed.length + (needsPad ? 1 : 0);
  const out = new Uint8Array(2 + contentLen);
  out[0] = 0x02;        // INTEGER
  out[1] = contentLen;
  let idx = 2;
  if (needsPad) { out[idx++] = 0; }
  out.set(trimmed, idx);
  return out;
}

// ── Base64 helpers ───────────────────────────────────────────────────

function bytesToBase64(bytes) {
  let s = '';
  for (const b of bytes) s += String.fromCharCode(b);
  return btoa(s);
}

// ── Session token ────────────────────────────────────────────────────

function saveSession(token, userId, keyId) {
  localStorage.setItem(LS_TOKEN,   token);
  localStorage.setItem(LS_USER_ID, userId);
  if (keyId) localStorage.setItem(LS_KEY_ID, keyId);
}
function loadSession() {
  return {
    token:  localStorage.getItem(LS_TOKEN),
    userId: localStorage.getItem(LS_USER_ID),
    keyId:  localStorage.getItem(LS_KEY_ID),
  };
}
function clearSession() {
  localStorage.removeItem(LS_TOKEN);
  localStorage.removeItem(LS_USER_ID);
  localStorage.removeItem(LS_KEY_ID);
}

// ── API ──────────────────────────────────────────────────────────────

async function api(path, opts = {}) {
  const headers = Object.assign({ 'Accept': 'application/json' }, opts.headers || {});
  const { token } = loadSession();
  if (token) headers['Authorization'] = 'Bearer ' + token;
  if (opts.body) headers['Content-Type'] = 'application/json';
  const r = await fetch('/v1' + path, Object.assign({}, opts, { headers }));
  const text = await r.text();
  let body = null;
  if (text) { try { body = JSON.parse(text); } catch { /* leave as null */ } }
  if (!r.ok) {
    const msg = (body && (body.message || body.error)) || text || ('http ' + r.status);
    const err = new Error(msg);
    err.status = r.status;
    throw err;
  }
  return body;
}

// ── High-level flows ────────────────────────────────────────────────

/**
 * Silent registration. Idempotent — if the browser already has a
 * session, returns the existing one. Otherwise generates a fresh
 * keypair, registers with the server, and stores both.
 */
async function registerIfNeeded() {
  const existing = loadSession();
  if (existing.token) return existing;

  const keypair = await generateKeypair();
  const pubRaw  = await exportPublicKeyRaw(keypair.publicKey);
  // key_id is metadata for the server — an opaque identifier so a
  // future multi-device UI can label keys. We pick "web-" + random.
  const keyId = 'web-' + crypto.randomUUID();

  const resp = await api('/onboard/register', {
    method: 'POST',
    body: JSON.stringify({
      public_key: bytesToBase64(pubRaw),
      key_id: keyId,
    }),
  });

  await saveKeypair(keypair);
  saveSession(resp.session_token, resp.user_id, keyId);

  return {
    token:  resp.session_token,
    userId: resp.user_id,
    keyId,
  };
}

/**
 * Pull a fresh challenge, sign the nonce with the browser's key, and
 * return `{ nonce, signature_b64, expires_at }`. The caller renders
 * the QR and schedules the next refresh.
 */
async function signedChallenge() {
  const kp = await loadKeypair();
  if (!kp) throw new Error('no keypair — call registerIfNeeded first');

  const resp = await api('/onboard/challenge', { method: 'POST' });

  const nonceBytes = new TextEncoder().encode(resp.nonce);
  const rawSig = new Uint8Array(await crypto.subtle.sign(
    { name: 'ECDSA', hash: 'SHA-256' },
    kp.privateKey,
    nonceBytes
  ));
  const derSig = rawSignatureToDer(rawSig);

  return {
    nonce:         resp.nonce,
    signature_b64: bytesToBase64(derSig),
    expires_at:    resp.expires_at,
  };
}

/** Current server-side status: { id, status, verified_at, event }. */
async function fetchMe() {
  return await api('/me');
}

// Export as a module-shaped global so the individual pages can consume
// this without a build step.
window.BoxFraise = {
  registerIfNeeded,
  signedChallenge,
  fetchMe,
  loadSession,
  clearSession,
};
