-- =============================================================
-- 0010 — Businesses gain location + slug.
--
-- location: freeform address for display on the business page.
-- slug:     URL-safe identifier; the business page is served at
--           /business/{slug}. Nullable at first so existing rows
--           don't have to be backfilled by the migration itself;
--           the operator can populate as businesses onboard.
-- =============================================================

BEGIN;

ALTER TABLE businesses
    ADD COLUMN location TEXT,
    ADD COLUMN slug     TEXT UNIQUE;

COMMIT;
