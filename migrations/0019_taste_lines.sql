-- =============================================================
-- 0019 — Taste lines.
--
-- The pool the strawberry draws from. Every row is one completion of
-- the prompt "for better taste:", attributed to whoever said it — a
-- business that paid to be in the pool, or a person.
--
-- Scanning a strawberry returns one of these at random. That is the
-- whole mechanic: the platform holds no feed, and a line reaches
-- somebody only because they went and found a sticker.
--
-- Note the direction of the read, which is the opposite of
-- `submissions` (0018):
--
--   submissions   public WRITE, admin-only READ. Correspondence.
--   taste_lines   admin-only WRITE, public READ. Published matter.
--
-- Only 'published' rows are publicly visible. A draft is the editor
-- thinking, and nobody scanning a strawberry should receive it.
--
-- There is deliberately no public INSERT here. Businesses will
-- eventually pay to enter the pool and verified members will enter it
-- free, but both of those route through a person deciding — the same
-- rule as the magazine. Until that exists the editor writes the pool
-- themselves, which is enough to run the mechanic.
-- =============================================================

BEGIN;

CREATE TABLE taste_lines (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- The completion. Short on purpose: it is a line, not a paragraph,
    -- and it is read on a phone held up to a sticker.
    body          TEXT NOT NULL,

    -- Who said it. Shown with the line, always.
    attribution   TEXT NOT NULL,

    -- Where it came from. 'editor' is the seeded pool; the other two
    -- are what the mechanic becomes once there is anyone to fill them.
    source        TEXT NOT NULL DEFAULT 'editor',

    status        TEXT NOT NULL DEFAULT 'draft',

    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    published_at  TIMESTAMPTZ,
    created_by_admin_id UUID REFERENCES admins (id) ON DELETE SET NULL,

    CONSTRAINT taste_lines_status_valid
        CHECK (status IN ('draft', 'published')),
    CONSTRAINT taste_lines_source_valid
        CHECK (source IN ('editor', 'business', 'member')),
    CONSTRAINT taste_lines_body_len
        CHECK (char_length(body) BETWEEN 1 AND 120),
    CONSTRAINT taste_lines_attribution_len
        CHECK (char_length(attribution) BETWEEN 1 AND 80),
    -- A published row must record when. Nothing else reads this yet,
    -- but a pool with no publication dates cannot be reported on later.
    CONSTRAINT taste_lines_published_has_date
        CHECK (status <> 'published' OR published_at IS NOT NULL)
);

-- The draw: published rows only.
CREATE INDEX idx_taste_lines_published
    ON taste_lines (created_at DESC)
    WHERE status = 'published';

ALTER TABLE taste_lines ENABLE ROW LEVEL SECURITY;
ALTER TABLE taste_lines FORCE  ROW LEVEL SECURITY;

-- Published lines are public. This is the one table in the schema
-- meant to be read by anyone — it is the published matter.
CREATE POLICY taste_lines_public_select ON taste_lines
    FOR SELECT
    USING (
        status = 'published'
        OR current_setting('app.is_admin', true) = 'true'
    );

-- Writes are the editor's alone. There is no public INSERT.
CREATE POLICY taste_lines_admin_insert ON taste_lines
    FOR INSERT
    WITH CHECK (current_setting('app.is_admin', true) = 'true');

CREATE POLICY taste_lines_admin_update ON taste_lines
    FOR UPDATE
    USING (current_setting('app.is_admin', true) = 'true')
    WITH CHECK (current_setting('app.is_admin', true) = 'true');

CREATE POLICY taste_lines_admin_delete ON taste_lines
    FOR DELETE
    USING (current_setting('app.is_admin', true) = 'true');

GRANT SELECT, INSERT, UPDATE, DELETE ON taste_lines TO bf_app;

COMMIT;
