-- =============================================================
-- 0011 — Events gain a poster_url.
--
-- Path or URL to a designed PDF for the event — a shareable/
-- printable artifact that carries tone the plain-text description
-- can't. Nullable; only events that have a designed poster get a
-- link rendered on the /events page.
-- =============================================================

BEGIN;

ALTER TABLE events
    ADD COLUMN poster_url TEXT;

COMMIT;
