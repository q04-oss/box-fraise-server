-- =============================================================
-- 0008 — Prune non-MVP schema.
--
-- The MVP is: someone shows up at an event, opens fraise.box/pass
-- on their phone, authenticates via the P-256 device-key flow, and
-- an admin scans them in with the /admin scanner. Every domain
-- module built for the earlier identity-network vision that isn't
-- on that path is being retired. This migration drops its DB
-- surface.
--
-- Dropped:
--   * hair_profiles, model_requests, model_invitations
--     (hair-student model-matching flow — src/domain/modeling)
--   * personal_items (personal-calendar CRUD — src/domain/schedule)
--   * celestial columns on the surviving tables
--     (Meeus astronomy layer — src/celestial)
--
-- Retained: users, admins, sessions, device_keys, challenges,
-- events (+ questions column), consultations, cards, staff,
-- salons, salon_appointments, services, professional_licenses,
-- social_verifications, audit_events. The consultation-to-card
-- flow stays because Tier-2 verification remains part of the
-- vision even though it isn't in the first-event MVP.
-- =============================================================

BEGIN;

-- Hair-student model-matching flow (0006).
DROP TABLE IF EXISTS model_invitations CASCADE;
DROP TABLE IF EXISTS model_requests   CASCADE;
DROP TABLE IF EXISTS hair_profiles    CASCADE;

-- Personal calendar (0003).
DROP TABLE IF EXISTS personal_items   CASCADE;

-- Celestial columns (0004). No SQL/API code reads them any more.
ALTER TABLE events
    DROP COLUMN IF EXISTS moon_phase,
    DROP COLUMN IF EXISTS moon_longitude_deg,
    DROP COLUMN IF EXISTS sun_longitude_deg;

ALTER TABLE salon_appointments
    DROP COLUMN IF EXISTS moon_phase,
    DROP COLUMN IF EXISTS moon_longitude_deg,
    DROP COLUMN IF EXISTS sun_longitude_deg;

ALTER TABLE consultation_requests
    DROP COLUMN IF EXISTS moon_phase,
    DROP COLUMN IF EXISTS moon_longitude_deg,
    DROP COLUMN IF EXISTS sun_longitude_deg;

COMMIT;
