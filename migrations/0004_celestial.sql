-- =============================================================
-- 0004 — Celestial time.
--
-- Every schedule entry carries the sky it happened under. Columns
-- are computed by the service layer on INSERT (and on UPDATE if
-- starts_at changes), so historical rows retain their original
-- celestial context even if the ephemeris improves later.
--
-- Three raw values are stored; the interpretive layer (zodiac
-- sign, element, modality, moon-phase name, season) is derived
-- from those on the way out. This keeps the DB honest (raw
-- astronomical values) and lets the API layer evolve labels
-- without a migration.
-- =============================================================


-- personal_items
ALTER TABLE personal_items
    ADD COLUMN moon_phase          DOUBLE PRECISION,
    ADD COLUMN moon_longitude_deg  DOUBLE PRECISION,
    ADD COLUMN sun_longitude_deg   DOUBLE PRECISION;

-- salon_appointments
ALTER TABLE salon_appointments
    ADD COLUMN moon_phase          DOUBLE PRECISION,
    ADD COLUMN moon_longitude_deg  DOUBLE PRECISION,
    ADD COLUMN sun_longitude_deg   DOUBLE PRECISION;

-- consultation_requests — celestial context is on the created appointment,
-- but requests are also schedule-shaped so people can see when they
-- submitted (which sign the sun was in during the ask). Optional.
ALTER TABLE consultation_requests
    ADD COLUMN moon_phase          DOUBLE PRECISION,
    ADD COLUMN moon_longitude_deg  DOUBLE PRECISION,
    ADD COLUMN sun_longitude_deg   DOUBLE PRECISION;

-- events already have starts_at; celestial columns join them to the
-- rest of the schedule model. Boy Band Auditions can now be described
-- as "under a waning crescent in Cancer" — real information about the
-- moment.
ALTER TABLE events
    ADD COLUMN moon_phase          DOUBLE PRECISION,
    ADD COLUMN moon_longitude_deg  DOUBLE PRECISION,
    ADD COLUMN sun_longitude_deg   DOUBLE PRECISION;

-- No backfill here — the service layer computes on next update, and
-- new rows populate immediately. Old rows return null celestial in
-- the API response, which the UI treats as "sky unknown" (rare
-- enough not to warrant a migration-time compute).
