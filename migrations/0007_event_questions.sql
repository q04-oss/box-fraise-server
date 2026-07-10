-- Events acquire a first-class questions array. Each event has its
-- own set of discussion questions used for the ice-breaker; the full
-- archive across all events becomes a browsable record on the site
-- (/questions) — a history of what has been asked, so future organisers
-- can see the shape of prior conversations.

BEGIN;

ALTER TABLE events
    ADD COLUMN questions TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[];

COMMIT;
