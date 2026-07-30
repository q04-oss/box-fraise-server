-- =============================================================
-- 0013 — Sticker hosts, and sites.
--
-- Two changes, both consequences of the same decision: stickers are
-- no longer stuck to things nobody agreed to.
--
-- 1. stickers.host
--    Free text naming the shop, café or venue that agreed to host
--    the sticker. Nullable: the column arrives after rows already
--    exist, and a sticker in a genuinely public place may have no
--    host to name. Where it is set, the map says so — which is the
--    point. A project whose thesis is consensual accountability
--    should not grow by marking property without consent.
--
--    Deliberately NOT a foreign key to businesses. A host is a name
--    written on a pin, not a directory listing; requiring a
--    businesses row would mean onboarding a café before you could
--    put a sticker in it.
--
-- 2. sites
--    Somewhere you can stand and see a strawberry through your
--    phone's camera. Sites are not stickers: there is nothing
--    physical to photograph and no submission path, so they get
--    their own table rather than a `kind` column on stickers, which
--    would have left sticker_photos pointing at rows that can never
--    have photos.
--
--    The interaction is entirely client-side. The server publishes
--    where a site is; the browser decides whether the visitor is
--    close enough. No position is ever sent here, which is why
--    there is no "check-in" table and no user column anywhere in
--    this migration.
-- =============================================================

BEGIN;

ALTER TABLE stickers
    ADD COLUMN host TEXT;

CREATE TABLE sites (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug          TEXT NOT NULL UNIQUE,
    label         TEXT NOT NULL,
    -- What someone reads before they set off, and again when they
    -- arrive. Optional.
    blurb         TEXT,
    latitude      DOUBLE PRECISION NOT NULL,
    longitude     DOUBLE PRECISION NOT NULL,
    sort_order    INTEGER NOT NULL DEFAULT 0,
    published     BOOLEAN NOT NULL DEFAULT true,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT sites_lat_range  CHECK (latitude  BETWEEN  -90 AND  90),
    CONSTRAINT sites_lng_range  CHECK (longitude BETWEEN -180 AND 180),
    CONSTRAINT sites_slug_shape CHECK (slug ~ '^[a-z0-9]+(-[a-z0-9]+)*$')
);

CREATE INDEX idx_sites_published_sort
    ON sites (published, sort_order DESC, label ASC);

ALTER TABLE sites ENABLE ROW LEVEL SECURITY;
ALTER TABLE sites FORCE  ROW LEVEL SECURITY;

-- Same shape as stickers_public_select / businesses_public_select:
-- published rows are world readable, admins additionally see drafts.
CREATE POLICY sites_public_select ON sites
    FOR SELECT
    USING (published OR current_setting('app.is_admin', true) = 'true');

CREATE POLICY sites_admin_write ON sites
    FOR ALL
    USING (current_setting('app.is_admin', true) = 'true')
    WITH CHECK (current_setting('app.is_admin', true) = 'true');

-- No DELETE: a site is unpublished, not removed, so the slug in
-- anyone's browser history keeps resolving.
GRANT SELECT, INSERT, UPDATE ON sites TO bf_app;

COMMIT;
