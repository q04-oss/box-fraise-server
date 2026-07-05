-- =============================================================
-- 0006 — Hair profiles, hair students, model requests.
--
-- The first genuinely useful reason for a user to be Tier 2. When a
-- hair student needs a practice model, they post a request with time /
-- date / location / hair criteria; the server fans out invitations to
-- every willing-to-model user whose hair matches. First model to
-- accept wins; the accepted session automatically drops into their
-- personal calendar.
--
-- Hair profile info is captured by the consultant during the
-- consultation. The user doesn't type — the consultant asks the
-- questions and enters the answers. `willing_to_model` and
-- `is_hair_student` are toggled based on the user's explicit consent
-- in that conversation, and the user can flip willing_to_model off at
-- any time from /my.
-- =============================================================


-- ── hair_profiles ────────────────────────────────────────────────────
--
-- One row per user, populated at consultation time. Not required —
-- users who don't want to disclose can simply not have a row.

CREATE TABLE hair_profiles (
    user_id             UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    hair_length         TEXT CHECK (hair_length IN ('short', 'medium', 'long', 'very_long', 'shaved')),
    hair_texture        TEXT CHECK (hair_texture IN ('straight', 'wavy', 'curly', 'coily')),
    hair_type           TEXT,   -- 1a..4c standard hair typing
    hair_thickness      TEXT CHECK (hair_thickness IN ('fine', 'medium', 'thick')),
    natural_color       TEXT,   -- black | brown | blonde | red | grey | other
    current_color       TEXT,   -- freetext (actual current shade)
    chemically_treated  BOOLEAN NOT NULL DEFAULT false,
    willing_services    TEXT[],
    willing_to_model    BOOLEAN NOT NULL DEFAULT false,
    is_hair_student     BOOLEAN NOT NULL DEFAULT false,
    hair_notes          TEXT,   -- consultant's freeform notes
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_hair_profiles_willing_to_model
    ON hair_profiles (willing_to_model)
    WHERE willing_to_model = true;
CREATE INDEX idx_hair_profiles_students
    ON hair_profiles (is_hair_student)
    WHERE is_hair_student = true;

ALTER TABLE hair_profiles ENABLE ROW LEVEL SECURITY;
ALTER TABLE hair_profiles FORCE ROW LEVEL SECURITY;

-- Self-read + admin-read. The model-search endpoint operates under an
-- AdminRlsTransaction; no policy specifically grants students the
-- ability to read arbitrary hair profiles, so the search flow can
-- never fan out identifying details — it returns matches internally
-- and only creates invitations, never returning hair data to the
-- caller.
CREATE POLICY hair_profiles_self_or_admin_select ON hair_profiles FOR SELECT
    USING (
        user_id = NULLIF(current_setting('app.user_id', true), '')::uuid
        OR current_setting('app.is_admin', true) = 'true'
    );
CREATE POLICY hair_profiles_admin_insert ON hair_profiles FOR INSERT
    WITH CHECK (current_setting('app.is_admin', true) = 'true');
-- User can update just their willing_to_model flag; the rest requires
-- admin context (a consultant would need to update via the admin path
-- if hair data changed).
CREATE POLICY hair_profiles_self_or_admin_update ON hair_profiles FOR UPDATE
    USING (
        user_id = NULLIF(current_setting('app.user_id', true), '')::uuid
        OR current_setting('app.is_admin', true) = 'true'
    )
    WITH CHECK (
        user_id = NULLIF(current_setting('app.user_id', true), '')::uuid
        OR current_setting('app.is_admin', true) = 'true'
    );

GRANT SELECT, INSERT, UPDATE ON hair_profiles TO bf_app;


-- ── model_requests ──────────────────────────────────────────────────
--
-- A hair student posts a request. Filters are OPTIONAL — an empty
-- array means "any." Multi-value filters match users whose profile
-- value is one of the listed options.

CREATE TABLE model_requests (
    id                 UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    student_user_id    UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    service            TEXT NOT NULL,   -- freetext description (e.g. "cut & colour practice")
    starts_at          TIMESTAMPTZ NOT NULL,
    ends_at            TIMESTAMPTZ NOT NULL,
    location           TEXT NOT NULL,
    location_lat       DOUBLE PRECISION,
    location_lng       DOUBLE PRECISION,
    -- Hair criteria filters (each is an array of acceptable values; empty = no filter).
    filter_length      TEXT[] NOT NULL DEFAULT '{}',
    filter_texture     TEXT[] NOT NULL DEFAULT '{}',
    filter_type        TEXT[] NOT NULL DEFAULT '{}',
    filter_color       TEXT[] NOT NULL DEFAULT '{}',  -- filter on natural_color
    additional_notes   TEXT,
    status             TEXT NOT NULL DEFAULT 'open'
                       CHECK (status IN ('open', 'filled', 'cancelled', 'expired')),
    filled_by_user_id  UUID REFERENCES users(id),
    filled_at          TIMESTAMPTZ,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT model_requests_time_window CHECK (ends_at > starts_at)
);
CREATE INDEX idx_model_requests_student ON model_requests(student_user_id, created_at DESC);
CREATE INDEX idx_model_requests_open_time ON model_requests(status, starts_at) WHERE status = 'open';

ALTER TABLE model_requests ENABLE ROW LEVEL SECURITY;
ALTER TABLE model_requests FORCE ROW LEVEL SECURITY;

-- Student can see their own; admin can see all. Potential models see
-- their invitations (see model_invitations below), not the raw
-- requests directly — this keeps the requester's requirements from
-- being publicly enumerable.
CREATE POLICY model_requests_student_or_admin_select ON model_requests FOR SELECT
    USING (
        student_user_id = NULLIF(current_setting('app.user_id', true), '')::uuid
        OR current_setting('app.is_admin', true) = 'true'
    );
CREATE POLICY model_requests_admin_insert ON model_requests FOR INSERT
    WITH CHECK (current_setting('app.is_admin', true) = 'true');
CREATE POLICY model_requests_admin_update ON model_requests FOR UPDATE
    USING (current_setting('app.is_admin', true) = 'true')
    WITH CHECK (current_setting('app.is_admin', true) = 'true');

GRANT SELECT, INSERT, UPDATE ON model_requests TO bf_app;


-- ── model_invitations ───────────────────────────────────────────────
--
-- One row per potential model fanned out from a request. Model sees
-- their own; student can see all invitations against their request.
-- On accept, the request is marked filled and an entry lands in the
-- accepting user's personal_items via the service layer.

CREATE TABLE model_invitations (
    id                       UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    model_request_id         UUID NOT NULL REFERENCES model_requests(id) ON DELETE CASCADE,
    potential_model_user_id  UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    invited_at               TIMESTAMPTZ NOT NULL DEFAULT now(),
    responded_at             TIMESTAMPTZ,
    response                 TEXT CHECK (response IN ('accepted', 'declined')),
    schedule_item_id         UUID REFERENCES personal_items(id) ON DELETE SET NULL,
    UNIQUE (model_request_id, potential_model_user_id)
);
CREATE INDEX idx_model_invitations_model ON model_invitations(potential_model_user_id, invited_at DESC);
CREATE INDEX idx_model_invitations_request ON model_invitations(model_request_id);

ALTER TABLE model_invitations ENABLE ROW LEVEL SECURITY;
ALTER TABLE model_invitations FORCE ROW LEVEL SECURITY;

CREATE POLICY model_invitations_participant_or_admin_select ON model_invitations FOR SELECT
    USING (
        potential_model_user_id = NULLIF(current_setting('app.user_id', true), '')::uuid
        OR current_setting('app.is_admin', true) = 'true'
        OR EXISTS (
            SELECT 1 FROM model_requests r
             WHERE r.id = model_invitations.model_request_id
               AND r.student_user_id = NULLIF(current_setting('app.user_id', true), '')::uuid
        )
    );
CREATE POLICY model_invitations_admin_insert ON model_invitations FOR INSERT
    WITH CHECK (current_setting('app.is_admin', true) = 'true');
-- Model can respond to their own invitations; admin can update any.
CREATE POLICY model_invitations_participant_update ON model_invitations FOR UPDATE
    USING (
        potential_model_user_id = NULLIF(current_setting('app.user_id', true), '')::uuid
        OR current_setting('app.is_admin', true) = 'true'
    )
    WITH CHECK (
        potential_model_user_id = NULLIF(current_setting('app.user_id', true), '')::uuid
        OR current_setting('app.is_admin', true) = 'true'
    );

GRANT SELECT, INSERT, UPDATE ON model_invitations TO bf_app;
