-- =============================================================
-- 0014 — Sightings replace placed stickers.
--
-- The model inverted. Stickers are no longer placed by the operator
-- at known spots and then photographed: they are sold at events for
-- $5, stuck wherever the buyer likes, and the map is built from
-- people reporting the ones they come across.
--
-- So there is nothing for an operator to place, and no business
-- hosting anything. A sighting IS the pin. That collapses the old
-- two-table shape (stickers + sticker_photos) into one.
--
-- Dropping rather than migrating: 0012 and 0013 were never applied
-- to production, so there is no real data to preserve. Development
-- databases lose their seed rows, which is fine. The drops are at
-- the bottom, after the new table exists, so a failure leaves the
-- transaction rolled back with the old shape intact.
--
-- Coordinates come from the uploader tapping a map, not from the
-- device. That is a deliberate choice, not an oversight: a stated
-- location is a decision the person made, where a captured one is
-- data taken from them. It also means someone can add a sighting
-- later, at home, from a photo they took earlier.
-- =============================================================

BEGIN;

CREATE TABLE sightings (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    image_bytes     BYTEA   NOT NULL,
    content_type    TEXT    NOT NULL,
    byte_size       INTEGER NOT NULL,

    -- Where the person says they saw it.
    latitude        DOUBLE PRECISION NOT NULL,
    longitude       DOUBLE PRECISION NOT NULL,

    -- Both optional, both free text from an anonymous submitter.
    -- Never render either without escaping.
    caption         TEXT,
    submitter_name  TEXT,

    -- 'pending' until an admin approves. No 'rejected' state: a
    -- rejection DELETEs the row, because holding unwanted
    -- user-submitted image bytes indefinitely is a liability. The
    -- audit_events row records that the rejection happened.
    status          TEXT NOT NULL DEFAULT 'pending',

    submitted_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    reviewed_at     TIMESTAMPTZ,
    reviewed_by_admin_id UUID REFERENCES admins (id) ON DELETE SET NULL,

    CONSTRAINT sightings_status_valid
        CHECK (status IN ('pending', 'approved')),
    CONSTRAINT sightings_lat_range CHECK (latitude  BETWEEN  -90 AND  90),
    CONSTRAINT sightings_lng_range CHECK (longitude BETWEEN -180 AND 180),
    -- 8 MiB. Mirrors MAX_IMAGE_BYTES in the service layer and the
    -- DefaultBodyLimit on the route.
    CONSTRAINT sightings_size_cap
        CHECK (byte_size > 0 AND byte_size <= 8 * 1024 * 1024),
    CONSTRAINT sightings_size_matches
        CHECK (byte_size = octet_length(image_bytes)),
    CONSTRAINT sightings_content_type_allowed
        CHECK (content_type IN ('image/jpeg', 'image/png', 'image/webp')),
    CONSTRAINT sightings_caption_len
        CHECK (caption IS NULL OR char_length(caption) <= 280),
    CONSTRAINT sightings_submitter_len
        CHECK (submitter_name IS NULL OR char_length(submitter_name) <= 60)
);

-- Public map read: approved sightings, newest first.
CREATE INDEX idx_sightings_status_submitted
    ON sightings (status, submitted_at DESC);

-- Moderation queue: oldest pending first.
CREATE INDEX idx_sightings_pending
    ON sightings (submitted_at ASC)
    WHERE status = 'pending';

ALTER TABLE sightings ENABLE ROW LEVEL SECURITY;
ALTER TABLE sightings FORCE  ROW LEVEL SECURITY;

-- Only approved sightings are publicly visible. An unapproved one is
-- invisible to every non-admin read path, which is what makes the
-- open INSERT below safe: a submission is not published content.
CREATE POLICY sightings_public_select ON sightings
    FOR SELECT
    USING (
        status = 'approved'
        OR current_setting('app.is_admin', true) = 'true'
    );

-- THE open door, carried over from sticker_photos. Any request — no
-- bearer token, no user context — may insert, but ONLY as 'pending'.
--
-- `status = 'pending'` in WITH CHECK is the load-bearing clause: it
-- stops a submitter self-publishing by posting status 'approved'. Do
-- not relax it. The service layer also hardcodes the status rather
-- than reading it from the request, so this is the second of two
-- independent barriers.
--
-- The reviewed_* columns must be empty on insert so a submitter
-- cannot fabricate a review trail.
CREATE POLICY sightings_public_insert ON sightings
    FOR INSERT
    WITH CHECK (
        status = 'pending'
        AND reviewed_at IS NULL
        AND reviewed_by_admin_id IS NULL
    );

CREATE POLICY sightings_admin_all ON sightings
    FOR ALL
    USING (current_setting('app.is_admin', true) = 'true')
    WITH CHECK (current_setting('app.is_admin', true) = 'true');

GRANT SELECT, INSERT, UPDATE, DELETE ON sightings TO bf_app;

-- Pending counter for the anti-flood guard, same reasoning as the
-- sticker one it replaces: pending rows are invisible to non-admin
-- SELECT, so a public request cannot count them, and neither
-- widening the SELECT policy nor handing a public path admin context
-- is acceptable. SECURITY DEFINER with a pinned search_path exposes
-- exactly one integer.
--
-- Note this is now a GLOBAL count rather than per-sticker, because
-- there is nothing to scope it to. It bounds how much unreviewed
-- image data can accumulate; it is not an abuse control, since a
-- determined flooder can still fill the queue and, in doing so, keep
-- honest submissions out until the queue is cleared. Moderation
-- throughput is the real control.
CREATE OR REPLACE FUNCTION bf_pending_sighting_count()
RETURNS INTEGER
LANGUAGE sql
SECURITY DEFINER
SET search_path = public, pg_temp
STABLE
AS $$
    SELECT COUNT(*)::int FROM sightings WHERE status = 'pending'
$$;

REVOKE ALL     ON FUNCTION bf_pending_sighting_count() FROM PUBLIC;
GRANT  EXECUTE ON FUNCTION bf_pending_sighting_count() TO bf_app;

-- ── Retire the placed-sticker model ──────────────────────────────
--
-- sticker_photos goes first: it has the foreign key.
DROP TABLE IF EXISTS sticker_photos;
DROP TABLE IF EXISTS stickers;
DROP FUNCTION IF EXISTS bf_sticker_pending_photo_count(UUID);

COMMIT;
