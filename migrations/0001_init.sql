-- =============================================================
-- Box Fraise MVP — initial schema
--
-- Two-role enforcement model (the most important property of this file):
--   - `postgres` (owner): runs this migration, owns every object,
--     bypasses RLS. The app never connects as this role.
--   - `bf_app`  (runtime): the app's only connection identity. No
--     BYPASSRLS. Sees only the rows its policies permit and can only
--     execute the verbs explicitly granted at the bottom of this file.
-- Every table also has `FORCE ROW LEVEL SECURITY`, so RLS still applies
-- even if a connection happens to be the owner role. Belt and suspenders.
-- =============================================================

-- ── GUC contract (read before changing any policy) ───────────────────
--
-- Two transaction-local GUCs drive RLS:
--   * `app.user_id`  — UUID of the authenticated user, set by
--     RlsTransaction::begin via set_config('app.user_id', $1, true).
--   * `app.is_admin` — literal 'true' under an AdminRlsTransaction.
--
-- Both are read via current_setting('<key>', true). The trailing `true`
-- means "missing GUC → NULL" instead of raising. That's deliberate:
--   - NULL <comparator> anything → NULL → the policy is not satisfied
--     → the row is invisible. So a forgotten context yields zero rows,
--     never an error and never a leak.
--
-- Never write the literal empty string into `app.user_id` from the
-- application — but watch for this Postgres quirk: once set_config has
-- ever touched this GUC on a connection, the slot is allocated, and
-- after the transaction commits the slot reverts to the empty string
-- (NOT NULL). Cast ''::uuid raises. The policies below therefore wrap
-- every read in NULLIF(..., '')::uuid so a "previously set, now
-- cleared" GUC behaves the same as "never set" — both yield NULL,
-- which fails every comparison and hides every row.
--
-- Both GUCs MUST be set with `is_local = true` so they live only inside
-- the current transaction. Without that, they outlive the request and
-- leak across pool connections — the historic source of "/me returns
-- empty" and cross-user data exposure bugs.

CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- ── Tables ────────────────────────────────────────────────────────────

CREATE TABLE admins (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email           TEXT UNIQUE NOT NULL,
    password_hash   TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE events (
    id                       UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name                     TEXT NOT NULL,
    description              TEXT,
    host_name                TEXT NOT NULL,
    latitude                 DOUBLE PRECISION NOT NULL,
    longitude                DOUBLE PRECISION NOT NULL,
    address                  TEXT NOT NULL,
    starts_at                TIMESTAMPTZ NOT NULL,
    ends_at                  TIMESTAMPTZ NOT NULL,
    published                BOOLEAN NOT NULL DEFAULT false,
    created_by_admin_id      UUID NOT NULL REFERENCES admins(id),
    created_at               TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT events_time_window_valid CHECK (ends_at > starts_at)
);
CREATE INDEX idx_events_published_starts_at
    ON events(published, starts_at DESC);

CREATE TABLE users (
    id                       UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    status                   TEXT NOT NULL DEFAULT 'pending'
                             CHECK (status IN ('pending', 'verified')),
    registered_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    verified_at              TIMESTAMPTZ,
    verified_at_event_id     UUID REFERENCES events(id),
    verified_by_admin_id     UUID REFERENCES admins(id),
    CONSTRAINT users_verified_state_consistent CHECK (
        (status = 'pending'   AND verified_at IS NULL
                              AND verified_at_event_id IS NULL
                              AND verified_by_admin_id IS NULL)
     OR (status = 'verified'  AND verified_at IS NOT NULL
                              AND verified_at_event_id IS NOT NULL
                              AND verified_by_admin_id IS NOT NULL)
    )
);
CREATE INDEX idx_users_status_registered
    ON users(status, registered_at DESC);

CREATE TABLE device_keys (
    -- 1:1 with users (MVP: one device per identity). Future multi-device
    -- support adds a key_id-scoped PK and a secondary index.
    user_id                  UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    public_key               BYTEA NOT NULL,   -- SEC1 uncompressed, 65 bytes
    key_id                   TEXT NOT NULL,    -- opaque client-side identifier
    registered_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE challenges (
    nonce                    TEXT PRIMARY KEY,
    user_id                  UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    issued_at                TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at               TIMESTAMPTZ NOT NULL,
    used_at                  TIMESTAMPTZ
);
CREATE INDEX idx_challenges_user_id ON challenges(user_id);
CREATE INDEX idx_challenges_expires_at ON challenges(expires_at);

CREATE TABLE user_sessions (
    -- We store only sha256(token); the raw token never touches disk.
    token_hash               TEXT PRIMARY KEY,
    user_id                  UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at               TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_user_sessions_user_id ON user_sessions(user_id);

CREATE TABLE admin_sessions (
    token_hash               TEXT PRIMARY KEY,
    admin_id                 UUID NOT NULL REFERENCES admins(id) ON DELETE CASCADE,
    created_at               TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at               TIMESTAMPTZ NOT NULL
);
CREATE INDEX idx_admin_sessions_expires_at ON admin_sessions(expires_at);

CREATE TABLE audit_events (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    actor_type      TEXT NOT NULL CHECK (actor_type IN ('user', 'admin', 'system')),
    actor_id        UUID,
    action          TEXT NOT NULL,
    target          TEXT,
    metadata        JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_audit_events_action_created
    ON audit_events(action, created_at DESC);

-- ── Append-only enforcement on audit_events ──────────────────────────
--
-- The application also writes audit rows on a separate connection
-- (outside any request transaction) so the audit survives a request
-- rollback. The trigger below makes the table tamper-evident at the DB
-- level — any UPDATE or DELETE raises, regardless of role.

CREATE OR REPLACE FUNCTION bf_prevent_modification()
RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'audit_events is append-only — % not permitted',
        lower(TG_OP);
END $$;

CREATE TRIGGER audit_events_no_update
    BEFORE UPDATE ON audit_events
    FOR EACH ROW EXECUTE FUNCTION bf_prevent_modification();
CREATE TRIGGER audit_events_no_delete
    BEFORE DELETE ON audit_events
    FOR EACH ROW EXECUTE FUNCTION bf_prevent_modification();

-- ── Row Level Security ───────────────────────────────────────────────

-- admins
-- Reads are admin-scoped. The seed-admin bootstrap (server-side) opens
-- an AdminRlsTransaction, looks up by email, and inserts if missing —
-- the INSERT policy below permits that. There is no admin signup
-- endpoint; password rotation is a follow-up flow.
ALTER TABLE admins ENABLE ROW LEVEL SECURITY;
ALTER TABLE admins FORCE ROW LEVEL SECURITY;
CREATE POLICY admins_admin_select ON admins FOR SELECT
    USING (current_setting('app.is_admin', true) = 'true');
CREATE POLICY admins_bootstrap_insert ON admins FOR INSERT
    WITH CHECK (current_setting('app.is_admin', true) = 'true');

-- events
-- Public (unauthenticated and any-user) reads see only `published`.
-- Admin reads see everything. Admins are the only writers.
ALTER TABLE events ENABLE ROW LEVEL SECURITY;
ALTER TABLE events FORCE ROW LEVEL SECURITY;
CREATE POLICY events_public_select ON events FOR SELECT
    USING (published OR current_setting('app.is_admin', true) = 'true');
CREATE POLICY events_admin_insert ON events FOR INSERT
    WITH CHECK (current_setting('app.is_admin', true) = 'true');
CREATE POLICY events_admin_update ON events FOR UPDATE
    USING (current_setting('app.is_admin', true) = 'true')
    WITH CHECK (current_setting('app.is_admin', true) = 'true');

-- users
-- Self-read and self-update via `app.user_id`. Admins see and update
-- every row. Registration runs before any user context exists, so the
-- INSERT policy is wide-open: a fresh pending row can always be
-- created; it is the admin scan that promotes it to verified.
ALTER TABLE users ENABLE ROW LEVEL SECURITY;
ALTER TABLE users FORCE ROW LEVEL SECURITY;
CREATE POLICY users_self_or_admin_select ON users FOR SELECT
    USING (
        id = NULLIF(current_setting('app.user_id', true), '')::uuid
        OR current_setting('app.is_admin', true) = 'true'
    );
CREATE POLICY users_self_or_admin_update ON users FOR UPDATE
    USING (
        id = NULLIF(current_setting('app.user_id', true), '')::uuid
        OR current_setting('app.is_admin', true) = 'true'
    )
    WITH CHECK (
        id = NULLIF(current_setting('app.user_id', true), '')::uuid
        OR current_setting('app.is_admin', true) = 'true'
    );
CREATE POLICY users_register_insert ON users FOR INSERT
    WITH CHECK (true);
CREATE POLICY users_admin_delete ON users FOR DELETE
    USING (current_setting('app.is_admin', true) = 'true');

-- device_keys
-- 1:1 with users, scoped identically. INSERT permitted during
-- registration before any user context exists.
ALTER TABLE device_keys ENABLE ROW LEVEL SECURITY;
ALTER TABLE device_keys FORCE ROW LEVEL SECURITY;
CREATE POLICY device_keys_self_or_admin_select ON device_keys FOR SELECT
    USING (
        user_id = NULLIF(current_setting('app.user_id', true), '')::uuid
        OR current_setting('app.is_admin', true) = 'true'
    );
CREATE POLICY device_keys_register_insert ON device_keys FOR INSERT
    WITH CHECK (true);
CREATE POLICY device_keys_admin_delete ON device_keys FOR DELETE
    USING (current_setting('app.is_admin', true) = 'true');

-- challenges
-- INSERT scoped to the authed user (their own challenge). The admin scan
-- reads + marks used_at — only admins. Deletes (for prune) are admin.
ALTER TABLE challenges ENABLE ROW LEVEL SECURITY;
ALTER TABLE challenges FORCE ROW LEVEL SECURITY;
CREATE POLICY challenges_self_insert ON challenges FOR INSERT
    WITH CHECK (user_id = NULLIF(current_setting('app.user_id', true), '')::uuid);
CREATE POLICY challenges_admin_select ON challenges FOR SELECT
    USING (current_setting('app.is_admin', true) = 'true');
CREATE POLICY challenges_admin_update ON challenges FOR UPDATE
    USING (current_setting('app.is_admin', true) = 'true')
    WITH CHECK (current_setting('app.is_admin', true) = 'true');
CREATE POLICY challenges_admin_delete ON challenges FOR DELETE
    USING (current_setting('app.is_admin', true) = 'true');

-- user_sessions / admin_sessions — bootstrap problem
--
-- The auth middleware has to resolve `Bearer <token>` → user_id (or
-- admin_id) before any user/admin context can possibly be set. The
-- session tables therefore need a narrow read path that does not depend
-- on the very GUC we're trying to populate.
--
-- We resolve this by granting bf_app SELECT on these two tables under
-- a wide-open USING(true) policy. The application is the audit
-- boundary: the *only* code path that reads these tables is the
-- middleware token-resolution step, and it always filters by
-- token_hash. Treat this as a documented, narrow privilege — not a
-- general read pattern.
--
-- INSERTs cover registration / login (no context yet). DELETEs cover
-- the periodic prune of expired admin sessions and stale pending-user
-- sessions.
ALTER TABLE user_sessions ENABLE ROW LEVEL SECURITY;
ALTER TABLE user_sessions FORCE ROW LEVEL SECURITY;
CREATE POLICY user_sessions_lookup ON user_sessions FOR SELECT
    USING (true);
CREATE POLICY user_sessions_register_insert ON user_sessions FOR INSERT
    WITH CHECK (true);
CREATE POLICY user_sessions_admin_delete ON user_sessions FOR DELETE
    USING (current_setting('app.is_admin', true) = 'true');

ALTER TABLE admin_sessions ENABLE ROW LEVEL SECURITY;
ALTER TABLE admin_sessions FORCE ROW LEVEL SECURITY;
CREATE POLICY admin_sessions_lookup ON admin_sessions FOR SELECT
    USING (true);
CREATE POLICY admin_sessions_login_insert ON admin_sessions FOR INSERT
    WITH CHECK (true);
CREATE POLICY admin_sessions_admin_delete ON admin_sessions FOR DELETE
    USING (current_setting('app.is_admin', true) = 'true');

-- audit_events
-- Wide-open INSERT — auditing fires from many code paths (admin scan,
-- registration, login, prune). Reads are admin-only. UPDATE / DELETE
-- are caught by the trigger above and also lack a grant.
ALTER TABLE audit_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE audit_events FORCE ROW LEVEL SECURITY;
CREATE POLICY audit_events_insert ON audit_events FOR INSERT
    WITH CHECK (true);
CREATE POLICY audit_events_admin_select ON audit_events FOR SELECT
    USING (current_setting('app.is_admin', true) = 'true');

-- ── Runtime role grants ──────────────────────────────────────────────
--
-- bf_app gets only the verbs the application actually needs. No UPDATE
-- or DELETE on audit_events — the trigger would also catch a stray
-- attempt, but missing the grant fails earlier and louder.

GRANT SELECT, INSERT, UPDATE                ON admins         TO bf_app;
GRANT SELECT, INSERT, UPDATE                ON events         TO bf_app;
GRANT SELECT, INSERT, UPDATE, DELETE        ON users          TO bf_app;
GRANT SELECT, INSERT, UPDATE, DELETE        ON device_keys    TO bf_app;
GRANT SELECT, INSERT, UPDATE, DELETE        ON challenges     TO bf_app;
GRANT SELECT, INSERT, DELETE                ON user_sessions  TO bf_app;
GRANT SELECT, INSERT, DELETE                ON admin_sessions TO bf_app;
GRANT SELECT, INSERT                        ON audit_events   TO bf_app;
