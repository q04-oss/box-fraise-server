-- =============================================================
-- 0026 — The channel that opens after a pairing.
--
-- Two people met at a run, both said yes, and three days passed. This
-- is what they got. Nothing before that point has a channel: a pairing
-- that is waiting, deciding, lapsed or closed cannot carry a message,
-- and the policies below are what say so rather than a handler.
--
-- The server cannot read any of it. Messages arrive as ciphertext and
-- are stored as ciphertext — the key is derived in the two browsers
-- from an ECDH exchange and never leaves them. What the operator can
-- see is that two numbers exchanged something, how much of it, and
-- when. That is metadata and it is unavoidable; the words are not.
--
-- The trade that buys: no history on a second device and no recovery.
-- The key lives where the credential lives, which is the phone, and
-- /join already says to keep it.
--
-- Static ECDH, so there is no forward secrecy: somebody who gets a
-- member's private key can read everything that member ever received.
-- Ratcheting would fix that and is a much larger machine. Worth doing
-- if this ever carries something that matters more than it does now.
--
-- Reading requires the pairing to still be open. Blocking therefore
-- takes the whole conversation away from both people rather than
-- leaving one side holding a transcript of somebody who left.
-- =============================================================

BEGIN;

-- ── Whether two people may talk at all ─────────────────────────────
-- SECURITY DEFINER because `pairings` is under RLS: the policies below
-- run as the member, who can see their own pairings, but the function
-- keeps this one question in one place rather than repeating the
-- lifecycle logic in every policy.
CREATE OR REPLACE FUNCTION bf_pairing_is_open_for(pid UUID, uid UUID)
RETURNS BOOLEAN
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
    SELECT EXISTS (
        SELECT 1 FROM pairings
         WHERE id = pid
           AND opened_at IS NOT NULL
           AND closed_at IS NULL
           AND (lower_user_id = uid OR upper_user_id = uid)
    );
$$;

-- ── Public keys ────────────────────────────────────────────────────
-- Raw P-256 ECDH, SEC1 uncompressed: 0x04 || X(32) || Y(32).
CREATE TABLE member_keys (
    user_id    UUID PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    public_key BYTEA NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT member_keys_sec1 CHECK (
        octet_length(public_key) = 65 AND get_byte(public_key, 0) = 4
    )
);

ALTER TABLE member_keys ENABLE ROW LEVEL SECURITY;
ALTER TABLE member_keys FORCE  ROW LEVEL SECURITY;

-- Your own key, or the key of somebody you have an open channel with.
-- A public key is an identifier, so it is not public.
CREATE POLICY member_keys_self_or_peer_select ON member_keys
    FOR SELECT
    USING (
        user_id = NULLIF(current_setting('app.user_id', true), '')::uuid
        OR EXISTS (
            SELECT 1 FROM pairings p
             WHERE p.opened_at IS NOT NULL
               AND p.closed_at IS NULL
               AND (
                    (p.lower_user_id = member_keys.user_id AND p.upper_user_id = NULLIF(current_setting('app.user_id', true), '')::uuid)
                 OR (p.upper_user_id = member_keys.user_id AND p.lower_user_id = NULLIF(current_setting('app.user_id', true), '')::uuid)
               )
        )
    );

-- You publish your own, and only your own.
CREATE POLICY member_keys_self_insert ON member_keys
    FOR INSERT
    WITH CHECK (user_id = NULLIF(current_setting('app.user_id', true), '')::uuid);

-- Replacing a key abandons every conversation you had, because the
-- shared secrets were derived from the old one. Allowed, because a
-- phone can be lost, but it is not a small act.
CREATE POLICY member_keys_self_update ON member_keys
    FOR UPDATE
    USING (user_id = NULLIF(current_setting('app.user_id', true), '')::uuid)
    WITH CHECK (user_id = NULLIF(current_setting('app.user_id', true), '')::uuid);

-- ── Messages ───────────────────────────────────────────────────────
CREATE TABLE messages (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    pairing_id UUID NOT NULL REFERENCES pairings (id) ON DELETE CASCADE,
    sender_id  UUID NOT NULL REFERENCES users (id)    ON DELETE CASCADE,

    -- AES-GCM. The server has neither key nor plaintext.
    ciphertext BYTEA NOT NULL,
    iv         BYTEA NOT NULL,

    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    -- 12 bytes is the GCM nonce size; anything else is a client bug.
    CONSTRAINT messages_iv_len CHECK (octet_length(iv) = 12),
    -- Roughly 6 KB of text once the tag and encoding are accounted for.
    CONSTRAINT messages_size CHECK (
        octet_length(ciphertext) BETWEEN 17 AND 8192
    )
);

CREATE INDEX idx_messages_pairing ON messages (pairing_id, created_at);

ALTER TABLE messages ENABLE ROW LEVEL SECURITY;
ALTER TABLE messages FORCE  ROW LEVEL SECURITY;

-- Both halves of the channel, and only while it is open.
CREATE POLICY messages_open_channel_select ON messages
    FOR SELECT
    USING (bf_pairing_is_open_for(pairing_id, NULLIF(current_setting('app.user_id', true), '')::uuid));

-- You send as yourself, into a channel that is open to you.
CREATE POLICY messages_open_channel_insert ON messages
    FOR INSERT
    WITH CHECK (
        sender_id = NULLIF(current_setting('app.user_id', true), '')::uuid
        AND bf_pairing_is_open_for(pairing_id, NULLIF(current_setting('app.user_id', true), '')::uuid)
    );

-- No UPDATE and no DELETE for anybody, including admins. A message is
-- said or it is not; the way a conversation ends is that the pairing
-- closes and the channel goes with it.

GRANT SELECT, INSERT, UPDATE ON member_keys TO bf_app;
GRANT SELECT, INSERT ON messages TO bf_app;
GRANT EXECUTE ON FUNCTION bf_pairing_is_open_for(UUID, UUID) TO bf_app;

COMMIT;
