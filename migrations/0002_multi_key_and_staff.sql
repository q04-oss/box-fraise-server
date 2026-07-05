-- =============================================================
-- 0002 — Multi-key per user + staff (stylists/admins/managers)
--        + salons.
--
-- Two structural shifts encoded here:
--
-- 1. Identity is no longer 1:1 with a single device. A user is an
--    identity that can bind multiple keys — a browser today, an iOS
--    Secure Enclave later, a Box Fraise hardware card after that.
--    Recovery is via any surviving key. No stored PII required.
--
-- 2. Elevated roles (stylist, admin, manager) are not a separate
--    identity space. A staff member IS a user with an attached staff
--    row. Same auth flow, same keys, same portraits (they get haircuts
--    too). This keeps Box Fraise's identifier space sovereign: no
--    dependency on a cosmetology-board license number or any external
--    registry. When Box Fraise itself becomes credentialing
--    infrastructure, everyone's already in Box Fraise's namespace.
-- =============================================================


-- ── Multi-key per user ──────────────────────────────────────────────
--
-- Previously: device_keys.user_id was the primary key, enforcing one
-- device per user. Now: each key gets a stable id; user_id becomes a
-- non-unique FK; a user can bind as many keys as they want.

ALTER TABLE device_keys DROP CONSTRAINT device_keys_pkey;
ALTER TABLE device_keys ADD COLUMN id UUID PRIMARY KEY DEFAULT gen_random_uuid();
CREATE INDEX idx_device_keys_user_id ON device_keys(user_id);

-- A single piece of key material shouldn't belong to two users. This
-- prevents key-reuse / key-collision confusion at the auth layer.
CREATE UNIQUE INDEX idx_device_keys_public_key ON device_keys(public_key);

-- Existing RLS policies on device_keys still work: they scope by
-- user_id, which is unchanged. No policy updates required.


-- ── Salons ──────────────────────────────────────────────────────────
--
-- A staff row can be scoped to a specific salon (the stylist works
-- there). Independents get NULL. Publicly readable so the eventual
-- /locations page and iOS app map can list them.

CREATE TABLE salons (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name            TEXT NOT NULL,
    address         TEXT,
    latitude        DOUBLE PRECISION,
    longitude       DOUBLE PRECISION,
    opened_at       TIMESTAMPTZ,
    closed_at       TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE salons ENABLE ROW LEVEL SECURITY;
ALTER TABLE salons FORCE ROW LEVEL SECURITY;

CREATE POLICY salons_public_select ON salons FOR SELECT
    USING (true);
CREATE POLICY salons_admin_insert ON salons FOR INSERT
    WITH CHECK (current_setting('app.is_admin', true) = 'true');
CREATE POLICY salons_admin_update ON salons FOR UPDATE
    USING (current_setting('app.is_admin', true) = 'true')
    WITH CHECK (current_setting('app.is_admin', true) = 'true');

GRANT SELECT, INSERT, UPDATE ON salons TO bf_app;


-- ── Staff ───────────────────────────────────────────────────────────
--
-- staff.user_id references users.id — a staff member IS a user. The
-- shared identifier is Box Fraise's UUID, not any external license
-- number. Any professional-license paperwork lives in a separate
-- optional table (see professional_licenses below), used for internal
-- verification only, never as an identifier.
--
-- Promotion flow (implemented at the service layer, not the schema):
-- an existing staff member with can_promote_others=true co-signs a
-- promotion event with the promotee's own device signature; both
-- signatures land in audit_events, and the resulting staff row
-- records promoted_by_user_id. Recursive trust building from the
-- seed admin outward.

CREATE TABLE staff (
    user_id                UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    role                   TEXT NOT NULL
                           CHECK (role IN ('stylist', 'admin', 'manager')),
    active_at_salon_id     UUID REFERENCES salons(id),
    hired_at               TIMESTAMPTZ NOT NULL DEFAULT now(),
    terminated_at          TIMESTAMPTZ,
    can_verify_others      BOOLEAN NOT NULL DEFAULT true,
    can_promote_others     BOOLEAN NOT NULL DEFAULT false,
    promoted_by_user_id    UUID REFERENCES users(id),
    promoted_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_staff_salon ON staff(active_at_salon_id);
CREATE INDEX idx_staff_role  ON staff(role);

ALTER TABLE staff ENABLE ROW LEVEL SECURITY;
ALTER TABLE staff FORCE ROW LEVEL SECURITY;

-- A user can see their own staff record. Admins can see all.
CREATE POLICY staff_self_or_admin_select ON staff FOR SELECT
    USING (
        user_id = NULLIF(current_setting('app.user_id', true), '')::uuid
        OR current_setting('app.is_admin', true) = 'true'
    );

-- Only admin context can promote (insert) or update.
CREATE POLICY staff_admin_insert ON staff FOR INSERT
    WITH CHECK (current_setting('app.is_admin', true) = 'true');
CREATE POLICY staff_admin_update ON staff FOR UPDATE
    USING (current_setting('app.is_admin', true) = 'true')
    WITH CHECK (current_setting('app.is_admin', true) = 'true');

GRANT SELECT, INSERT, UPDATE ON staff TO bf_app;


-- ── Professional licenses (optional, internal reference only) ───────
--
-- If the salon needs to record that a stylist holds an Alberta AIT
-- cosmetology licence, it lives here as a *fact about them*, not as
-- their identifier. The identifier is always the users.id UUID.

CREATE TABLE professional_licenses (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    authority       TEXT NOT NULL,      -- e.g. "Alberta AIT"
    license_number  TEXT NOT NULL,      -- external ID, kept opaque
    trade           TEXT,               -- e.g. "cosmetology", "barbering"
    issued_at       TIMESTAMPTZ,
    expires_at      TIMESTAMPTZ,
    recorded_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(authority, license_number)
);
CREATE INDEX idx_professional_licenses_user_id
    ON professional_licenses(user_id);

ALTER TABLE professional_licenses ENABLE ROW LEVEL SECURITY;
ALTER TABLE professional_licenses FORCE ROW LEVEL SECURITY;

CREATE POLICY prof_lic_self_or_admin_select ON professional_licenses FOR SELECT
    USING (
        user_id = NULLIF(current_setting('app.user_id', true), '')::uuid
        OR current_setting('app.is_admin', true) = 'true'
    );
CREATE POLICY prof_lic_admin_insert ON professional_licenses FOR INSERT
    WITH CHECK (current_setting('app.is_admin', true) = 'true');
CREATE POLICY prof_lic_admin_update ON professional_licenses FOR UPDATE
    USING (current_setting('app.is_admin', true) = 'true')
    WITH CHECK (current_setting('app.is_admin', true) = 'true');
CREATE POLICY prof_lic_admin_delete ON professional_licenses FOR DELETE
    USING (current_setting('app.is_admin', true) = 'true');

GRANT SELECT, INSERT, UPDATE, DELETE ON professional_licenses TO bf_app;
