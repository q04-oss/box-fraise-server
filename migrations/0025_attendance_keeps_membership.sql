-- =============================================================
-- 0025 — A membership is kept, not got.
--
-- Turning up once opens the account. Turning up every month is what
-- keeps it able to post. Miss a month and the account is still yours —
-- nothing is deleted, nothing is forgotten, your posts stay up — but
-- posting stops until you come back.
--
-- That is the whole thesis stated as a permission. The platform is
-- downstream of the community, so access is a function of being in it,
-- and the way to be in it is to be somewhere at eight on a Sunday.
--
-- Attendance is recorded by an admin who is looking at the person: they
-- ask for a member number and type it in. No code to scan, no link to
-- forward, nothing that works from a sofa. It cannot be gamed remotely
-- without an admin lying, which is the same trust the sign-up already
-- rests on.
--
-- Enforcement is in the INSERT policy rather than a handler, via a
-- SECURITY DEFINER function — attendances is under RLS and a posting
-- member has no context to read it, exactly like
-- bf_pending_submission_count.
-- =============================================================

BEGIN;

CREATE TABLE attendances (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     UUID NOT NULL REFERENCES users (id)  ON DELETE CASCADE,
    event_id    UUID NOT NULL REFERENCES events (id) ON DELETE CASCADE,
    recorded_by_admin_id UUID REFERENCES admins (id) ON DELETE SET NULL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    -- Somebody can only be at a given run once. Marking them twice is
    -- an admin pressing the button again, not a second attendance.
    CONSTRAINT attendances_once_per_event UNIQUE (user_id, event_id)
);

CREATE INDEX idx_attendances_user_recent ON attendances (user_id, recorded_at DESC);

ALTER TABLE attendances ENABLE ROW LEVEL SECURITY;
ALTER TABLE attendances FORCE  ROW LEVEL SECURITY;

-- A member may see their own record — it is how they know when their
-- membership lapses. Admins see everything.
CREATE POLICY attendances_own_or_admin_select ON attendances
    FOR SELECT
    USING (
        user_id = NULLIF(current_setting('app.user_id', true), '')::uuid
        OR current_setting('app.is_admin', true) = 'true'
    );

-- Only an admin records attendance. A member cannot mark themselves
-- present, which is the point.
CREATE POLICY attendances_admin_insert ON attendances
    FOR INSERT
    WITH CHECK (current_setting('app.is_admin', true) = 'true');

CREATE POLICY attendances_admin_delete ON attendances
    FOR DELETE
    USING (current_setting('app.is_admin', true) = 'true');

-- Signing up is turning up: the verification that created the account
-- is that person's first attendance.
INSERT INTO attendances (user_id, event_id, recorded_by_admin_id, recorded_at)
SELECT id, verified_at_event_id, verified_by_admin_id, verified_at
  FROM users
 WHERE status = 'verified' AND verified_at_event_id IS NOT NULL;

-- ── The window ─────────────────────────────────────────────────────
-- 31 days: come on the first of the month and you have until the first
-- of the next. Changing this changes who may post, so change it here
-- and nowhere else — the service and the interface both read it from
-- this function rather than keeping their own copy.
CREATE OR REPLACE FUNCTION bf_member_is_current(uid UUID)
RETURNS BOOLEAN
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
    SELECT EXISTS (
        SELECT 1 FROM attendances
         WHERE user_id = uid
           AND recorded_at > now() - interval '31 days'
    );
$$;

CREATE OR REPLACE FUNCTION bf_member_last_seen(uid UUID)
RETURNS TIMESTAMPTZ
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
    SELECT MAX(recorded_at) FROM attendances WHERE user_id = uid;
$$;

-- ── Posting requires a current membership ──────────────────────────
DROP POLICY submissions_member_insert ON submissions;

CREATE POLICY submissions_member_insert ON submissions
    FOR INSERT
    WITH CHECK (
        status = 'pending'
        AND user_id = NULLIF(current_setting('app.user_id', true), '')::uuid
        AND bf_member_is_current(user_id)
    );

GRANT SELECT, INSERT, DELETE ON attendances TO bf_app;
GRANT EXECUTE ON FUNCTION bf_member_is_current(UUID) TO bf_app;
GRANT EXECUTE ON FUNCTION bf_member_last_seen(UUID) TO bf_app;

COMMIT;
