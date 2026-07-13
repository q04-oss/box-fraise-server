-- =============================================================
-- 0009 — Businesses directory.
--
-- Public list of businesses participating in the portrait program.
-- Businesses sign up by emailing the operator; the operator inserts
-- the row by hand. There is no self-serve business signup and no
-- business-user account — just a curated list rendered on the home
-- page and readable via GET /v1/businesses.
--
-- MVP surface:
--   name          required, display name in the directory
--   description   optional, one-line blurb
--   website       optional, external URL
--   published     admin toggle; false rows stay hidden without
--                 losing the record
--   sort_order    small integer to override the default alphabetical
--                 sort when a business should appear at the top
-- =============================================================

BEGIN;

CREATE TABLE businesses (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name          TEXT NOT NULL,
    description   TEXT,
    website       TEXT,
    sort_order    INTEGER NOT NULL DEFAULT 0,
    published     BOOLEAN NOT NULL DEFAULT true,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_businesses_published_sort
    ON businesses (published, sort_order DESC, name ASC);

ALTER TABLE businesses ENABLE ROW LEVEL SECURITY;
ALTER TABLE businesses FORCE  ROW LEVEL SECURITY;

CREATE POLICY businesses_public_select ON businesses
    FOR SELECT
    USING (published OR current_setting('app.is_admin', true) = 'true');

CREATE POLICY businesses_admin_write ON businesses
    FOR ALL
    USING (current_setting('app.is_admin', true) = 'true')
    WITH CHECK (current_setting('app.is_admin', true) = 'true');

GRANT SELECT, INSERT, UPDATE, DELETE ON businesses TO bf_app;

COMMIT;
