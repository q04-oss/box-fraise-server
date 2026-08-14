-- =============================================================
-- 0018 — Submissions: columns and photographs sent in for the
-- magazine.
--
-- This reopens a public write path, deliberately. 0017 closed the
-- last one when the sticker map went; this one exists because the
-- magazine needs contributions from people who do not have accounts,
-- and requiring a verified account to send in a column would mean
-- nobody can send one.
--
-- It is bounded the same way the old one was, with one difference
-- that makes it tighter:
--
--   * INSERT is open to anyone, but ONLY as status 'pending'.
--   * There is NO public SELECT policy at all. Not "only approved
--     rows are visible" — no non-admin read path exists. A submission
--     is private correspondence to the editor, and the platform never
--     publishes it automatically. Whatever ends up in the magazine
--     gets there because a person put it there.
--
-- Because nothing public can read this table, `INSERT ... RETURNING`
-- fails here with a 42501 — Postgres applies SELECT policies to the
-- returned row. The repository generates the id in Rust instead. See
-- CLAUDE.md.
--
-- A submission is a column, a photograph, or both. The CHECK below
-- enforces that it is at least one of them, and that the three image
-- columns are all present or all absent together.
--
-- Rejection DELETEs the row rather than marking it: holding on to
-- unwanted images and unpublished writing indefinitely is a liability
-- and a discourtesy. The audit_events row is the lasting record that
-- the submission existed and was declined.
-- =============================================================

BEGIN;

CREATE TABLE submissions (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- The column. Optional, because a submission may be a photograph
    -- with nothing written on it.
    title           TEXT,
    body            TEXT,

    -- The photograph. All three together or none of them.
    image_bytes     BYTEA,
    content_type    TEXT,
    byte_size       INTEGER,

    -- Free text from an anonymous sender. Never render either without
    -- escaping. `contact` exists so the editor can write back and
    -- credit the person; it is the only reason it is collected.
    submitter_name    TEXT,
    submitter_contact TEXT,

    status          TEXT NOT NULL DEFAULT 'pending',

    submitted_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    reviewed_at     TIMESTAMPTZ,
    reviewed_by_admin_id UUID REFERENCES admins (id) ON DELETE SET NULL,

    CONSTRAINT submissions_status_valid
        CHECK (status IN ('pending', 'accepted')),

    -- A submission has to be something.
    CONSTRAINT submissions_has_content
        CHECK (body IS NOT NULL OR image_bytes IS NOT NULL),

    -- The image columns move as one.
    CONSTRAINT submissions_image_consistent
        CHECK (
            (image_bytes IS NULL AND content_type IS NULL AND byte_size IS NULL)
            OR (image_bytes IS NOT NULL AND content_type IS NOT NULL AND byte_size IS NOT NULL)
        ),

    -- 8 MiB, mirroring MAX_IMAGE_BYTES in the service layer and the
    -- DefaultBodyLimit on the route.
    CONSTRAINT submissions_size_cap
        CHECK (byte_size IS NULL OR (byte_size > 0 AND byte_size <= 8 * 1024 * 1024)),
    CONSTRAINT submissions_size_matches
        CHECK (byte_size IS NULL OR byte_size = octet_length(image_bytes)),
    -- No SVG: it is a script container, not an image. No HEIC: only
    -- Safari renders it, and the page re-encodes to JPEG before upload.
    CONSTRAINT submissions_content_type_allowed
        CHECK (content_type IS NULL
               OR content_type IN ('image/jpeg', 'image/png', 'image/webp')),

    CONSTRAINT submissions_title_len
        CHECK (title IS NULL OR char_length(title) <= 140),
    CONSTRAINT submissions_body_len
        CHECK (body IS NULL OR char_length(body) <= 20000),
    CONSTRAINT submissions_name_len
        CHECK (submitter_name IS NULL OR char_length(submitter_name) <= 80),
    CONSTRAINT submissions_contact_len
        CHECK (submitter_contact IS NULL OR char_length(submitter_contact) <= 200)
);

-- The editor's queue: oldest pending first, so nothing waits forever.
CREATE INDEX idx_submissions_pending
    ON submissions (submitted_at ASC)
    WHERE status = 'pending';

CREATE INDEX idx_submissions_status_submitted
    ON submissions (status, submitted_at DESC);

ALTER TABLE submissions ENABLE ROW LEVEL SECURITY;
ALTER TABLE submissions FORCE  ROW LEVEL SECURITY;

-- Admins only. There is no public read of this table in any state --
-- that is what makes the open INSERT below safe. A submission is
-- correspondence, not published content.
CREATE POLICY submissions_admin_select ON submissions
    FOR SELECT
    USING (current_setting('app.is_admin', true) = 'true');

-- The open door. Any request -- no bearer token, no user context --
-- may insert, but ONLY as 'pending'.
--
-- `status = 'pending'` in WITH CHECK is the load-bearing clause: it
-- stops a sender marking their own submission accepted. Do not relax
-- it. The service layer also hardcodes the status rather than reading
-- it from the request, so this is the second of two barriers.
CREATE POLICY submissions_public_insert ON submissions
    FOR INSERT
    WITH CHECK (status = 'pending');

CREATE POLICY submissions_admin_update ON submissions
    FOR UPDATE
    USING (current_setting('app.is_admin', true) = 'true')
    WITH CHECK (current_setting('app.is_admin', true) = 'true');

CREATE POLICY submissions_admin_delete ON submissions
    FOR DELETE
    USING (current_setting('app.is_admin', true) = 'true');

-- Backpressure for the public submit path. A public transaction
-- cannot see pending rows, so it cannot count them itself; this reads
-- across the RLS boundary with a pinned search_path.
CREATE OR REPLACE FUNCTION bf_pending_submission_count()
RETURNS INTEGER
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
    SELECT COUNT(*)::int FROM submissions WHERE status = 'pending';
$$;

GRANT SELECT, INSERT, UPDATE, DELETE ON submissions TO bf_app;
GRANT EXECUTE ON FUNCTION bf_pending_submission_count() TO bf_app;

COMMIT;
