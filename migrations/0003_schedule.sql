-- =============================================================
-- 0003 — Scheduling
--
-- Introduces the schedule as the platform's activity backbone. Four
-- kinds of schedule entries, unified conceptually but split into
-- purpose-specific tables so each can enforce its own constraints:
--
--   * personal_items       — user's private calendar entries
--   * salon_appointments   — client + stylist + salon + service booking
--   * consultation_requests — commercial-advertising intake, which
--     when approved is bound to a created salon_appointment
--   * events               — Box Fraise-run happenings (already exists
--     from 0001, not touched here)
--
-- Plus services, the catalogue of bookable things.
-- =============================================================


-- ── services ─────────────────────────────────────────────────────────
--
-- The list of things a salon can offer. Bookable at an appointment.
-- Publicly readable (users need to see the catalogue), admin writable.

CREATE TABLE services (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name                TEXT NOT NULL,
    description         TEXT,
    duration_minutes    INTEGER NOT NULL CHECK (duration_minutes > 0),
    base_price_cents    INTEGER NOT NULL DEFAULT 0 CHECK (base_price_cents >= 0),
    active              BOOLEAN NOT NULL DEFAULT true,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_services_active ON services(active);

ALTER TABLE services ENABLE ROW LEVEL SECURITY;
ALTER TABLE services FORCE ROW LEVEL SECURITY;

CREATE POLICY services_public_select ON services FOR SELECT
    USING (true);
CREATE POLICY services_admin_insert ON services FOR INSERT
    WITH CHECK (current_setting('app.is_admin', true) = 'true');
CREATE POLICY services_admin_update ON services FOR UPDATE
    USING (current_setting('app.is_admin', true) = 'true')
    WITH CHECK (current_setting('app.is_admin', true) = 'true');

GRANT SELECT, INSERT, UPDATE ON services TO bf_app;

-- Starter catalogue. Admins can add / disable / edit from the admin
-- panel — this seed just makes sure the calendar has something to show
-- from day one.
INSERT INTO services (name, description, duration_minutes, base_price_cents) VALUES
    ('Haircut',                 'A cut for any hair length.',                       30, 4500),
    ('Cut & Blowout',           'Cut with a full blowout finish.',                  45, 6500),
    ('Colour',                  'Full-head colour service.',                        90, 12000),
    ('Consultation',            'Sit-down consultation before a colour or cut.',    30, 0),
    ('Portrait Update',         'A fresh portrait for your Box Fraise profile.',    15, 0),
    ('Commercial Consultation', 'Sit-down with a salon manager to discuss advertising with Box Fraise.',
                                                                                    60, 0);


-- ── personal_items ───────────────────────────────────────────────────
--
-- The user's private calendar. Strictly owner-scoped — even admins
-- cannot read personal items. This is a hard privacy line: your
-- calendar entries are yours, and only decrypt-able by you being
-- present in the request.

CREATE TABLE personal_items (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title           TEXT NOT NULL,
    notes           TEXT,
    starts_at       TIMESTAMPTZ NOT NULL,
    ends_at         TIMESTAMPTZ NOT NULL,
    is_all_day      BOOLEAN NOT NULL DEFAULT false,
    location        TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT personal_items_time_valid CHECK (ends_at >= starts_at)
);
CREATE INDEX idx_personal_items_user_starts
    ON personal_items(user_id, starts_at DESC);

ALTER TABLE personal_items ENABLE ROW LEVEL SECURITY;
ALTER TABLE personal_items FORCE ROW LEVEL SECURITY;

-- Owner-only. No admin escape hatch — a support admin cannot see a
-- user's calendar unless that user is present under RLS.
CREATE POLICY personal_items_owner_select ON personal_items FOR SELECT
    USING (user_id = NULLIF(current_setting('app.user_id', true), '')::uuid);
CREATE POLICY personal_items_owner_insert ON personal_items FOR INSERT
    WITH CHECK (user_id = NULLIF(current_setting('app.user_id', true), '')::uuid);
CREATE POLICY personal_items_owner_update ON personal_items FOR UPDATE
    USING (user_id = NULLIF(current_setting('app.user_id', true), '')::uuid)
    WITH CHECK (user_id = NULLIF(current_setting('app.user_id', true), '')::uuid);
CREATE POLICY personal_items_owner_delete ON personal_items FOR DELETE
    USING (user_id = NULLIF(current_setting('app.user_id', true), '')::uuid);

GRANT SELECT, INSERT, UPDATE, DELETE ON personal_items TO bf_app;


-- ── salon_appointments ───────────────────────────────────────────────
--
-- Structured booking: a client (user_id), a stylist (staff.user_id),
-- a salon, a service, a time. Status lifecycle covers the operational
-- reality of a salon day: scheduled → in_progress → completed
-- (or cancelled / no_show).

CREATE TABLE salon_appointments (
    id                      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id                 UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    stylist_user_id         UUID NOT NULL REFERENCES users(id),
    salon_id                UUID NOT NULL REFERENCES salons(id),
    service_id              UUID NOT NULL REFERENCES services(id),
    starts_at               TIMESTAMPTZ NOT NULL,
    ends_at                 TIMESTAMPTZ NOT NULL,
    status                  TEXT NOT NULL DEFAULT 'scheduled'
                            CHECK (status IN ('scheduled', 'in_progress', 'completed', 'cancelled', 'no_show')),
    staff_notes             TEXT,
    created_by_user_id      UUID NOT NULL REFERENCES users(id),
    created_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
    cancelled_at            TIMESTAMPTZ,
    cancelled_by_user_id    UUID REFERENCES users(id),
    cancellation_reason     TEXT,
    CONSTRAINT salon_appointments_time_valid CHECK (ends_at > starts_at)
);
CREATE INDEX idx_salon_appts_user       ON salon_appointments(user_id, starts_at DESC);
CREATE INDEX idx_salon_appts_stylist    ON salon_appointments(stylist_user_id, starts_at DESC);
CREATE INDEX idx_salon_appts_salon_time ON salon_appointments(salon_id, starts_at DESC);

ALTER TABLE salon_appointments ENABLE ROW LEVEL SECURITY;
ALTER TABLE salon_appointments FORCE ROW LEVEL SECURITY;

-- Visible to the client, the assigned stylist, and admins.
CREATE POLICY salon_appts_participant_or_admin_select ON salon_appointments FOR SELECT
    USING (
        user_id           = NULLIF(current_setting('app.user_id', true), '')::uuid
        OR stylist_user_id = NULLIF(current_setting('app.user_id', true), '')::uuid
        OR current_setting('app.is_admin', true) = 'true'
    );

-- v1: admins create appointments (walk-in / phone-in model). v2 will
-- add a self-book policy when availability is real.
CREATE POLICY salon_appts_admin_insert ON salon_appointments FOR INSERT
    WITH CHECK (current_setting('app.is_admin', true) = 'true');

-- Client can update to cancel their own; stylist can update status
-- for their own appointments; admins can update any.
CREATE POLICY salon_appts_participant_or_admin_update ON salon_appointments FOR UPDATE
    USING (
        user_id           = NULLIF(current_setting('app.user_id', true), '')::uuid
        OR stylist_user_id = NULLIF(current_setting('app.user_id', true), '')::uuid
        OR current_setting('app.is_admin', true) = 'true'
    )
    WITH CHECK (
        user_id           = NULLIF(current_setting('app.user_id', true), '')::uuid
        OR stylist_user_id = NULLIF(current_setting('app.user_id', true), '')::uuid
        OR current_setting('app.is_admin', true) = 'true'
    );

GRANT SELECT, INSERT, UPDATE ON salon_appointments TO bf_app;


-- ── consultation_requests ───────────────────────────────────────────
--
-- Intake for commercial-advertising conversations. A user (typically
-- a business owner who's been to the salon) requests a sit-down. Admin
-- (salon manager) reviews and either approves (creating a linked
-- salon_appointment with the Commercial Consultation service) or
-- declines. status transitions: pending → approved | declined.

CREATE TABLE consultation_requests (
    id                          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id                     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    salon_id                    UUID REFERENCES salons(id),
    business_name               TEXT NOT NULL,
    business_context            TEXT NOT NULL,
    preferred_windows           TEXT,
    status                      TEXT NOT NULL DEFAULT 'pending'
                                CHECK (status IN ('pending', 'approved', 'declined', 'completed')),
    approved_appointment_id     UUID REFERENCES salon_appointments(id),
    responded_at                TIMESTAMPTZ,
    responded_by_user_id        UUID REFERENCES users(id),
    response_note               TEXT,
    created_at                  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_consultation_requests_user   ON consultation_requests(user_id, created_at DESC);
CREATE INDEX idx_consultation_requests_status ON consultation_requests(status);

ALTER TABLE consultation_requests ENABLE ROW LEVEL SECURITY;
ALTER TABLE consultation_requests FORCE ROW LEVEL SECURITY;

CREATE POLICY consultations_requester_or_admin_select ON consultation_requests FOR SELECT
    USING (
        user_id = NULLIF(current_setting('app.user_id', true), '')::uuid
        OR current_setting('app.is_admin', true) = 'true'
    );
CREATE POLICY consultations_requester_insert ON consultation_requests FOR INSERT
    WITH CHECK (user_id = NULLIF(current_setting('app.user_id', true), '')::uuid);
CREATE POLICY consultations_admin_update ON consultation_requests FOR UPDATE
    USING (current_setting('app.is_admin', true) = 'true')
    WITH CHECK (current_setting('app.is_admin', true) = 'true');

GRANT SELECT, INSERT, UPDATE ON consultation_requests TO bf_app;
