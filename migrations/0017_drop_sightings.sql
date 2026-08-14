-- =============================================================
-- 0017 — Remove sightings.
--
-- The sticker map is gone, and with it the whole sticker mechanic:
-- stickers sold at the run, spotted in the world, photographed and
-- pinned. The homepage is now the question and the run club, and
-- nothing else.
--
-- This drops the last table anyone could write to without a token.
-- With it goes:
--
--   - the only unauthenticated INSERT in the schema, and the RLS
--     policy pinning it to status='pending'
--   - the only place the server stored bytes uploaded by the public
--   - the actor_type 'public', which now has nothing that uses it.
--     The CHECK constraint keeps it anyway: audit_events is
--     append-only, historical rows may already carry it, and
--     narrowing the constraint would invalidate them.
--
-- What remains that touches a camera is /scan, which recognises an
-- advertisement entirely in the browser and sends nothing anywhere.
-- =============================================================

BEGIN;

DROP TABLE IF EXISTS sightings;
DROP FUNCTION IF EXISTS bf_pending_sighting_count();

COMMIT;
