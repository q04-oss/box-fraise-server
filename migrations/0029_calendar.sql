-- The calendar.
--
-- A member should not have to chase their own schedule. A business on
-- the platform publishes it, and what it publishes is what is true —
-- no group chat, no text at eleven at night, no on-call ambiguity.
-- That is the whole product: the schedule is a published fact held
-- somewhere the employer does not own.
--
-- Two tables.
--
-- `employments` is the link 0028's strike needs and the calendar needs:
-- who works where. It is deliberately NOT modelled like `attendances`.
-- Attendance has no UPDATE grant because when somebody was somewhere is
-- a fact, not a field. Employment is the opposite — it is a status that
-- ends — so it is closable and deletable, and a member who leaves a job
-- can have it gone rather than accumulating an employment history that
-- outlives them.
--
-- `shifts` is what a business publishes. A published shift is binding:
-- it can be cancelled, and a cancellation is visible, but it cannot be
-- quietly rewritten into a different time. That asymmetry is the point.
-- A change is a cancellation plus a new shift, and both are on the
-- record.
--
-- On the record, but not forever. A year of shifts is a movement log —
-- where somebody was, when, how often — and after a strike month it is
-- exactly what an employer or a court would ask for. So shifts are
-- pruned on a TTL by src/maintenance.rs, the same way submissions are,
-- and nothing about them is written to audit_events, which is
-- append-only and could never give it back. Compare the note on
-- `messages` in 0026: some records are worse for existing.

CREATE TABLE employments (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id             UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    business_id         UUID NOT NULL REFERENCES businesses(id) ON DELETE CASCADE,
    started_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- NULL means currently employed there.
    ended_at            TIMESTAMPTZ,
    recorded_by_admin_id UUID REFERENCES admins(id),
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT employments_ends_after_start
        CHECK (ended_at IS NULL OR ended_at >= started_at)
);

-- One live employment per person per business. Two people can work at
-- the same place, and one person can work at two places; what is
-- refused is the same pairing recorded twice while still open.
CREATE UNIQUE INDEX idx_employments_live
    ON employments (user_id, business_id)
    WHERE ended_at IS NULL;
CREATE INDEX idx_employments_business ON employments (business_id) WHERE ended_at IS NULL;

CREATE TABLE shifts (
    id                   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id              UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    business_id          UUID NOT NULL REFERENCES businesses(id) ON DELETE CASCADE,
    starts_at            TIMESTAMPTZ NOT NULL,
    ends_at              TIMESTAMPTZ NOT NULL,
    published_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    published_by_admin_id UUID REFERENCES admins(id),
    -- A cancelled shift stays visible rather than disappearing, so a
    -- member can see that a thing they had planned around was taken
    -- away, and when.
    cancelled_at         TIMESTAMPTZ,
    cancelled_by_admin_id UUID REFERENCES admins(id),
    CONSTRAINT shifts_ends_after_start CHECK (ends_at > starts_at)
);

CREATE INDEX idx_shifts_user_time ON shifts (user_id, starts_at);

ALTER TABLE employments ENABLE ROW LEVEL SECURITY;
ALTER TABLE employments FORCE ROW LEVEL SECURITY;
ALTER TABLE shifts ENABLE ROW LEVEL SECURITY;
ALTER TABLE shifts FORCE ROW LEVEL SECURITY;

-- A member reads their own and nothing else. There is deliberately no
-- member write on either table: you do not schedule yourself, and you
-- do not declare where you work. Both are things somebody else states
-- about you, which is also what makes them worth anything.
CREATE POLICY employments_own_select ON employments FOR SELECT
    USING (
        user_id = NULLIF(current_setting('app.user_id', true), '')::uuid
        OR current_setting('app.is_admin', true) = 'true'
    );
CREATE POLICY employments_admin_write ON employments FOR INSERT
    WITH CHECK (current_setting('app.is_admin', true) = 'true');
CREATE POLICY employments_admin_update ON employments FOR UPDATE
    USING (current_setting('app.is_admin', true) = 'true');
CREATE POLICY employments_admin_delete ON employments FOR DELETE
    USING (current_setting('app.is_admin', true) = 'true');

CREATE POLICY shifts_own_select ON shifts FOR SELECT
    USING (
        user_id = NULLIF(current_setting('app.user_id', true), '')::uuid
        OR current_setting('app.is_admin', true) = 'true'
    );
CREATE POLICY shifts_admin_write ON shifts FOR INSERT
    WITH CHECK (current_setting('app.is_admin', true) = 'true');
-- UPDATE is granted for exactly one purpose: setting cancelled_at.
-- The times themselves are not editable, which is what "published" has
-- to mean for the product to be worth anything.
CREATE POLICY shifts_admin_cancel ON shifts FOR UPDATE
    USING (current_setting('app.is_admin', true) = 'true');
CREATE POLICY shifts_admin_delete ON shifts FOR DELETE
    USING (current_setting('app.is_admin', true) = 'true');

GRANT SELECT, INSERT, UPDATE, DELETE ON employments TO bf_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON shifts      TO bf_app;
