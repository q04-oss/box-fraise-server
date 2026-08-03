-- =============================================================
-- 0015 — Display names, and pairings.
--
-- Two people meet at an event, exchange a signed code in person, and
-- three days later are each asked separately whether they still want
-- to talk. A channel opens only if both say yes.
--
-- The cooling-off period is not a delay on a decision made at the
-- event; it is the reason no decision is made at the event at all.
-- Nothing can be agreed to on the spot, so nobody can be pressured
-- into agreeing.
--
-- See PAIRING.md for the full design. The rule that constrains
-- everything here: a declined pairing and an ignored one must be
-- indistinguishable to the other party, including in timing.
-- =============================================================

BEGIN;

-- ── Display names ────────────────────────────────────────────────
--
-- Until now a user was a device key and a status. Pairing needs
-- something to show the person you met — "someone you met at an
-- event" is not enough to recognise anyone by three days later.
--
-- Free text, chosen by the user, not verified and not unique. It is a
-- label, not an identifier: the identifier is still the key.
ALTER TABLE users
    ADD COLUMN display_name TEXT,
    ADD CONSTRAINT users_display_name_len
        CHECK (display_name IS NULL OR char_length(display_name) BETWEEN 1 AND 40);

-- ── Pairing nonces ───────────────────────────────────────────────
--
-- Mirrors `challenges`: short-lived, single-use. The QR carries ONLY
-- the nonce — never the initiator's user id — so photographing
-- someone's screen across a room teaches you nothing. The server
-- knows whose nonce it is because it issued it to an authenticated
-- caller.
CREATE TABLE pairing_nonces (
    nonce         TEXT PRIMARY KEY,
    initiator_id  UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    event_id      UUID REFERENCES events (id) ON DELETE SET NULL,
    issued_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at    TIMESTAMPTZ NOT NULL,
    used_at       TIMESTAMPTZ
);

CREATE INDEX idx_pairing_nonces_initiator ON pairing_nonces (initiator_id);

-- ── Pairings ─────────────────────────────────────────────────────
CREATE TABLE pairings (
    id                UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Ordered pair. Storing the two ids sorted, with a unique index,
    -- makes "one pairing per unordered pair" a database guarantee
    -- rather than an application convention — which kills the
    -- duplicate-row race when both people scan each other.
    lower_user_id     UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    upper_user_id     UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,

    event_id          UUID REFERENCES events (id) ON DELETE SET NULL,

    met_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    opens_at          TIMESTAMPTZ NOT NULL,   -- met_at + cooling period
    expires_at        TIMESTAMPTZ NOT NULL,   -- opens_at + decision window

    -- NULL = has not answered. Neither side may ever read the other's.
    lower_decision    TEXT,
    upper_decision    TEXT,
    lower_decided_at  TIMESTAMPTZ,
    upper_decided_at  TIMESTAMPTZ,

    opened_at         TIMESTAMPTZ,
    closed_at         TIMESTAMPTZ,
    closed_by         UUID REFERENCES users (id) ON DELETE SET NULL,

    CONSTRAINT pairings_ordered CHECK (lower_user_id < upper_user_id),
    CONSTRAINT pairings_window  CHECK (expires_at > opens_at),
    CONSTRAINT pairings_lower_decision_valid
        CHECK (lower_decision IS NULL OR lower_decision IN ('yes', 'no')),
    CONSTRAINT pairings_upper_decision_valid
        CHECK (upper_decision IS NULL OR upper_decision IN ('yes', 'no'))
);

CREATE UNIQUE INDEX pairings_unique_pair
    ON pairings (lower_user_id, upper_user_id);

CREATE INDEX idx_pairings_lower ON pairings (lower_user_id);
CREATE INDEX idx_pairings_upper ON pairings (upper_user_id);

ALTER TABLE pairing_nonces ENABLE ROW LEVEL SECURITY;
ALTER TABLE pairing_nonces FORCE  ROW LEVEL SECURITY;
ALTER TABLE pairings       ENABLE ROW LEVEL SECURITY;
ALTER TABLE pairings       FORCE  ROW LEVEL SECURITY;

-- A nonce is claimed by the *other* person, who by definition is not
-- the initiator and cannot be identified in advance. So the lookup
-- has to be possible for any authenticated user, exactly like the
-- sessions tables: the audit boundary is the application, and the
-- only read path filters by the nonce itself — which is unguessable
-- and short-lived.
CREATE POLICY pairing_nonces_select ON pairing_nonces
    FOR SELECT USING (true);

CREATE POLICY pairing_nonces_insert ON pairing_nonces
    FOR INSERT
    WITH CHECK (
        initiator_id = NULLIF(current_setting('app.user_id', true), '')::uuid
    );

CREATE POLICY pairing_nonces_burn ON pairing_nonces
    FOR UPDATE USING (true) WITH CHECK (true);

-- Participants only. This gets a caller to their own rows and no
-- further — but note it CANNOT hide the counterparty's decision
-- column, because RLS is row-level. That boundary lives in the
-- application: the read model has nowhere to put the other value.
CREATE POLICY pairings_participant_select ON pairings
    FOR SELECT
    USING (
        lower_user_id = NULLIF(current_setting('app.user_id', true), '')::uuid
        OR upper_user_id = NULLIF(current_setting('app.user_id', true), '')::uuid
    );

CREATE POLICY pairings_participant_insert ON pairings
    FOR INSERT
    WITH CHECK (
        (lower_user_id = NULLIF(current_setting('app.user_id', true), '')::uuid
         OR upper_user_id = NULLIF(current_setting('app.user_id', true), '')::uuid)
        AND lower_decision IS NULL
        AND upper_decision IS NULL
        AND opened_at IS NULL
        AND closed_at IS NULL
    );

CREATE POLICY pairings_participant_update ON pairings
    FOR UPDATE
    USING (
        lower_user_id = NULLIF(current_setting('app.user_id', true), '')::uuid
        OR upper_user_id = NULLIF(current_setting('app.user_id', true), '')::uuid
    )
    WITH CHECK (
        lower_user_id = NULLIF(current_setting('app.user_id', true), '')::uuid
        OR upper_user_id = NULLIF(current_setting('app.user_id', true), '')::uuid
    );

-- ── Reading the other person's name ──────────────────────────────
--
-- `users_self_or_admin_select` restricts a caller to their own row.
-- So a query that joins the counterparty's users row to fetch their
-- display name matches nothing, and — because it is an inner join —
-- silently drops the whole pairing. Under RLS a join is also a
-- filter; that is an easy and quiet way to lose rows.
--
-- Rather than widen the users policy (which would expose the peer's
-- entire row: status, verification, timestamps) this exposes exactly
-- one column, and only between two people who are actually paired.
-- The function guards itself, so it cannot be used to look up
-- arbitrary users by id.
--
-- Same tool and same reasoning as bf_pending_sighting_count: pinned
-- search_path, EXECUTE revoked from PUBLIC.
CREATE OR REPLACE FUNCTION bf_peer_display_name(p_viewer UUID, p_peer UUID)
RETURNS TEXT
LANGUAGE sql
SECURITY DEFINER
SET search_path = public, pg_temp
STABLE
AS $$
    SELECT u.display_name
      FROM users u
     WHERE u.id = p_peer
       AND EXISTS (
           SELECT 1 FROM pairings p
            WHERE (p.lower_user_id = p_viewer AND p.upper_user_id = p_peer)
               OR (p.upper_user_id = p_viewer AND p.lower_user_id = p_peer)
       )
$$;

REVOKE ALL     ON FUNCTION bf_peer_display_name(UUID, UUID) FROM PUBLIC;
GRANT  EXECUTE ON FUNCTION bf_peer_display_name(UUID, UUID) TO bf_app;

GRANT SELECT, INSERT, UPDATE, DELETE ON pairing_nonces TO bf_app;
GRANT SELECT, INSERT, UPDATE         ON pairings       TO bf_app;

COMMIT;
