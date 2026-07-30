-- =============================================================
-- 0012 — Sticker map.
--
-- Physical strawberry stickers are placed around the city as
-- advertising. This turns them into an engagement loop: the map on
-- /stickers shows where they are, people go find them, photograph
-- them, and the approved photos accumulate under each pin.
--
-- Two tables, two very different trust levels:
--
--   stickers        admin-curated, exactly like `businesses`. The
--                   operator places a sticker and inserts the row.
--                   No self-serve creation. `published` hides a pin
--                   without losing the record (e.g. sticker removed,
--                   or placed but not yet announced).
--
--   sticker_photos  PUBLIC WRITE. This is the only table in the
--                   schema an unauthenticated request can insert
--                   into, so the policies below are the whole
--                   security story. Read the INSERT policy carefully
--                   before touching it.
--
-- Photo bytes live in Postgres (BYTEA) rather than object storage.
-- Deliberate MVP call: no new infra, no credentials, and the bytes
-- inherit the RLS model instead of sitting in a bucket with its own
-- separate ACL story. The tradeoff is DB size — the byte cap is
-- enforced in the service layer (see stickers::service::MAX_IMAGE_
-- BYTES) and again by the CHECK constraint here, so a bug in one
-- layer cannot produce unbounded rows.
--
-- Prod note: the runtime container filesystem is ephemeral, so disk
-- was never an option for these. If DB growth becomes a problem the
-- migration path is to add a `storage_url` column, backfill, and
-- drop `image_bytes` — the API shape does not have to change.
-- =============================================================

BEGIN;

CREATE TABLE stickers (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- URL-safe identifier. The public API addresses stickers by slug
    -- rather than UUID so the pin URLs are shareable and legible.
    slug          TEXT NOT NULL UNIQUE,
    label         TEXT NOT NULL,
    -- Freeform "what to look for when you get there" — a window
    -- ledge, the back of a stop sign. The coordinates get you to the
    -- block; this gets you to the sticker.
    hint          TEXT,
    latitude      DOUBLE PRECISION NOT NULL,
    longitude     DOUBLE PRECISION NOT NULL,
    placed_on     DATE,
    sort_order    INTEGER NOT NULL DEFAULT 0,
    published     BOOLEAN NOT NULL DEFAULT true,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT stickers_lat_range  CHECK (latitude  BETWEEN  -90 AND  90),
    CONSTRAINT stickers_lng_range  CHECK (longitude BETWEEN -180 AND 180),
    CONSTRAINT stickers_slug_shape CHECK (slug ~ '^[a-z0-9]+(-[a-z0-9]+)*$')
);

CREATE INDEX idx_stickers_published_sort
    ON stickers (published, sort_order DESC, label ASC);

CREATE TABLE sticker_photos (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    sticker_id      UUID NOT NULL REFERENCES stickers (id) ON DELETE CASCADE,

    image_bytes     BYTEA NOT NULL,
    content_type    TEXT  NOT NULL,
    byte_size       INTEGER NOT NULL,

    -- Both optional and both free text from an anonymous submitter.
    -- Never render either without escaping.
    caption         TEXT,
    submitter_name  TEXT,

    -- 'pending' until an admin approves. There is no 'rejected'
    -- state on purpose: rejecting DELETEs the row, because holding
    -- unwanted user-submitted image bytes indefinitely is a
    -- liability, not an audit trail. The audit_events row records
    -- that a rejection happened.
    status          TEXT NOT NULL DEFAULT 'pending',

    submitted_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    reviewed_at     TIMESTAMPTZ,
    reviewed_by_admin_id UUID REFERENCES admins (id) ON DELETE SET NULL,

    CONSTRAINT sticker_photos_status_valid
        CHECK (status IN ('pending', 'approved')),
    -- 8 MiB. Mirrors MAX_IMAGE_BYTES in the service layer and the
    -- DefaultBodyLimit on the route. Belt, braces, and a second belt:
    -- the client downscales, the service checks, the DB refuses.
    CONSTRAINT sticker_photos_size_cap
        CHECK (byte_size > 0 AND byte_size <= 8 * 1024 * 1024),
    CONSTRAINT sticker_photos_size_matches
        CHECK (byte_size = octet_length(image_bytes)),
    CONSTRAINT sticker_photos_content_type_allowed
        CHECK (content_type IN ('image/jpeg', 'image/png', 'image/webp')),
    CONSTRAINT sticker_photos_caption_len
        CHECK (caption IS NULL OR char_length(caption) <= 280),
    CONSTRAINT sticker_photos_submitter_len
        CHECK (submitter_name IS NULL OR char_length(submitter_name) <= 60)
);

-- Public gallery read path: approved photos for one sticker, newest
-- first. Also the index the per-pin found-count aggregate rides on.
CREATE INDEX idx_sticker_photos_sticker_status
    ON sticker_photos (sticker_id, status, submitted_at DESC);

-- Moderation queue: oldest pending first.
CREATE INDEX idx_sticker_photos_pending
    ON sticker_photos (submitted_at ASC)
    WHERE status = 'pending';

ALTER TABLE stickers       ENABLE ROW LEVEL SECURITY;
ALTER TABLE stickers       FORCE  ROW LEVEL SECURITY;
ALTER TABLE sticker_photos ENABLE ROW LEVEL SECURITY;
ALTER TABLE sticker_photos FORCE  ROW LEVEL SECURITY;

-- Same shape as businesses_public_select: published rows are world
-- readable, admins additionally see drafts.
CREATE POLICY stickers_public_select ON stickers
    FOR SELECT
    USING (published OR current_setting('app.is_admin', true) = 'true');

CREATE POLICY stickers_admin_write ON stickers
    FOR ALL
    USING (current_setting('app.is_admin', true) = 'true')
    WITH CHECK (current_setting('app.is_admin', true) = 'true');

-- Only approved photos are publicly visible. An unapproved photo is
-- invisible to every non-admin read path, which is what makes the
-- open INSERT below safe: a submission is not published content.
CREATE POLICY sticker_photos_public_select ON sticker_photos
    FOR SELECT
    USING (
        status = 'approved'
        OR current_setting('app.is_admin', true) = 'true'
    );

-- THE open door. Any request — no bearer token, no user context —
-- may insert, but ONLY as 'pending', and ONLY against a sticker that
-- is currently visible to it.
--
-- `status = 'pending'` in WITH CHECK is the load-bearing clause: it
-- stops a submitter from self-publishing by posting status
-- 'approved'. Do not relax it. The service layer also hardcodes the
-- status rather than reading it from the request body, so this is
-- the second of two independent barriers.
--
-- The EXISTS subquery is evaluated with this same non-admin context,
-- so `stickers_public_select` applies inside it and unpublished
-- stickers are already invisible. The explicit `published` predicate
-- is redundant today and kept anyway — it states the intent locally
-- and survives someone loosening the sticker SELECT policy later.
--
-- The reviewed_* columns must be empty on insert so a submitter
-- cannot fabricate a review trail.
CREATE POLICY sticker_photos_public_insert ON sticker_photos
    FOR INSERT
    WITH CHECK (
        status = 'pending'
        AND reviewed_at IS NULL
        AND reviewed_by_admin_id IS NULL
        AND EXISTS (
            SELECT 1 FROM stickers s
             WHERE s.id = sticker_photos.sticker_id
               AND s.published
        )
    );

CREATE POLICY sticker_photos_admin_all ON sticker_photos
    FOR ALL
    USING (current_setting('app.is_admin', true) = 'true')
    WITH CHECK (current_setting('app.is_admin', true) = 'true');

-- Photo submission is the first action in the system taken by someone
-- who is neither an authenticated user, an admin, nor the server
-- itself. Rather than mislabel those audit rows 'system' (which would
-- make the trail lie about who acted), widen the actor vocabulary.
--
-- The append-only trigger on audit_events blocks row UPDATE/DELETE,
-- not DDL, so replacing the constraint is permitted for the owner
-- running this migration. Existing rows all satisfy the wider set.
ALTER TABLE audit_events
    DROP CONSTRAINT audit_events_actor_type_check;
ALTER TABLE audit_events
    ADD  CONSTRAINT audit_events_actor_type_check
         CHECK (actor_type IN ('user', 'admin', 'system', 'public'));

-- Pending-submission counter, for the anti-flood guard on the public
-- submit path.
--
-- Why a function: pending rows are invisible to non-admin SELECT (by
-- design — that's what makes the open INSERT safe), so a public
-- request cannot count them. The alternatives were worse: opening an
-- AdminRlsTransaction on an unauthenticated request would hand a
-- public code path admin context, and loosening the SELECT policy
-- would expose unmoderated submissions.
--
-- SECURITY DEFINER runs this as the table owner, bypassing RLS, so
-- the surface is kept to the absolute minimum: one integer, for one
-- sticker id, and no row contents. `search_path` is pinned because a
-- SECURITY DEFINER function with a mutable search_path is a
-- privilege-escalation vector. EXECUTE is revoked from PUBLIC and
-- granted only to bf_app.
CREATE OR REPLACE FUNCTION bf_sticker_pending_photo_count(p_sticker_id UUID)
RETURNS INTEGER
LANGUAGE sql
SECURITY DEFINER
SET search_path = public, pg_temp
STABLE
AS $$
    SELECT COUNT(*)::int
      FROM sticker_photos
     WHERE sticker_id = p_sticker_id
       AND status = 'pending'
$$;

REVOKE ALL     ON FUNCTION bf_sticker_pending_photo_count(UUID) FROM PUBLIC;
GRANT  EXECUTE ON FUNCTION bf_sticker_pending_photo_count(UUID) TO bf_app;

-- Grants. Note sticker_photos gets DELETE (reject drops the bytes)
-- while stickers does not — a placed sticker is unpublished, never
-- deleted, so its photo history survives.
GRANT SELECT, INSERT, UPDATE         ON stickers       TO bf_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON sticker_photos TO bf_app;

COMMIT;
