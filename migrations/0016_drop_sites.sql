-- =============================================================
-- 0016 — Remove strawberry sites.
--
-- Sites were places you could stand and see a strawberry through
-- your phone's camera, added in 0013. The run club replaced them:
-- the project now has one real-world thing it asks people to turn up
-- to, and it is on Whyte Avenue on a Saturday rather than scattered
-- across three pins.
--
-- Nothing is being preserved. Production never had a single row —
-- the sites were only ever placeholders in a development database —
-- so there is no data to migrate and no archive worth keeping.
--
-- Note what this does NOT remove: the camera itself. Scanning the
-- advertisement at /scan is a separate feature with its own reason
-- to exist. What goes is the geolocation proximity check, which
-- existed solely to decide whether someone was standing at a site.
-- With sites gone, the server no longer has any feature that asks a
-- browser where its user is.
-- =============================================================

BEGIN;

DROP TABLE IF EXISTS sites;

COMMIT;
