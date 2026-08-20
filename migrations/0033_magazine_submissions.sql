-- Anonymous writing sent in for the magazine.
--
-- The first edition has a chicken-and-egg problem: it is made from what
-- members wrote, and there are no members. This is the way out of it —
-- a stranger can send writing to the editor before there is anybody to
-- be a member.
--
-- Deliberately a separate table from runaway_answers, even though the
-- shape is nearly identical, because the policies are not:
--
--   runaway_answers      public INSERT, public SELECT once accepted.
--                        Accepted answers are shown on /runaway.
--   magazine_submissions public INSERT, NO public SELECT at all.
--                        This is for print. Nothing here is ever served
--                        to the web, in any state, to anybody but an
--                        admin.
--
-- That is the same write-without-read shape 0018 gave submissions
-- originally, and it means a bug in a handler cannot turn this into a
-- public wall. It also means INSERT ... RETURNING would fail with
-- 42501, so the id is generated in Rust — see the repository.
--
-- Text only. No image column and no image endpoint. An anonymous upload
-- path on a project associated with nudes is the highest-risk surface
-- available: CSAM and non-consensual images are criminal liability
-- rather than moderation policy, and none of the in-person verification
-- that makes the member path defensible applies to a stranger.
-- Photographs come from people who turned up.

CREATE TABLE magazine_submissions (
    id           UUID PRIMARY KEY,
    body         TEXT NOT NULL,
    status       TEXT NOT NULL DEFAULT 'pending',
    submitted_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    reviewed_at  TIMESTAMPTZ,
    reviewed_by_admin_id UUID REFERENCES admins(id),
    CONSTRAINT magazine_submissions_status_known
        CHECK (status IN ('pending', 'kept')),
    -- Long enough to be worth reading, short enough that the queue
    -- stays readable and one script cannot fill the table.
    CONSTRAINT magazine_submissions_length
        CHECK (char_length(body) BETWEEN 40 AND 20000)
);

CREATE INDEX idx_magazine_submissions_pending
    ON magazine_submissions (submitted_at ASC) WHERE status = 'pending';
CREATE INDEX idx_magazine_submissions_kept
    ON magazine_submissions (reviewed_at DESC) WHERE status = 'kept';

ALTER TABLE magazine_submissions ENABLE ROW LEVEL SECURITY;
ALTER TABLE magazine_submissions FORCE ROW LEVEL SECURITY;

-- Write, only ever as pending. Hardcoded in the repository as well;
-- this is the barrier that holds if that is ever edited carelessly.
CREATE POLICY magazine_submissions_public_insert ON magazine_submissions FOR INSERT
    WITH CHECK (status = 'pending');

-- No public SELECT policy of any kind. An admin transaction is the only
-- thing that can read this table.
CREATE POLICY magazine_submissions_admin_select ON magazine_submissions FOR SELECT
    USING (current_setting('app.is_admin', true) = 'true');
CREATE POLICY magazine_submissions_admin_update ON magazine_submissions FOR UPDATE
    USING (current_setting('app.is_admin', true) = 'true');
CREATE POLICY magazine_submissions_admin_delete ON magazine_submissions FOR DELETE
    USING (current_setting('app.is_admin', true) = 'true');

-- Backpressure, counted through a definer function because a public
-- transaction cannot see pending rows. Same shape as 0018 and 0032.
CREATE FUNCTION bf_pending_magazine_count() RETURNS INTEGER
    LANGUAGE sql SECURITY DEFINER SET search_path = public AS $$
    SELECT COUNT(*)::int FROM magazine_submissions WHERE status = 'pending';
$$;

GRANT SELECT, INSERT, UPDATE, DELETE ON magazine_submissions TO bf_app;
GRANT EXECUTE ON FUNCTION bf_pending_magazine_count() TO bf_app;

INSERT INTO schema_migrations (version) VALUES ('0033_magazine_submissions')
ON CONFLICT (version) DO NOTHING;
