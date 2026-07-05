-- =============================================================
-- 0005 — Consultations, identity cards, Box Fraise-originated identity.
--
-- Where the platform stops depending on external identity documents
-- and starts issuing its own credentials. A user becomes a Tier 2
-- member of the Box Fraise social layer through a private, unhurried
-- consultation conducted by a trained Box Fraise consultant. No
-- government ID is required or stored — the consultant's professional
-- judgment, signed cryptographically, is the credentialing act. The
-- resulting record + a physical identity card together are the
-- credential.
--
-- Same principle as declining to use the AIT license number for
-- staff: don't build your identity system on someone else's
-- identifier when you're trying to become the identifier.
-- =============================================================


-- ── staff schema additions ──────────────────────────────────────────
--
-- All stylists are trained to conduct identity consultations, but
-- training happens after hire. Record training completion + who
-- signed off. Derived permission: can_conduct_consultations =
-- (consultation_training_completed_at IS NOT NULL AND terminated_at IS NULL).
--
-- The trainer_user_id forms an auditable chain of trust: every
-- consultant traces back through their trainer, all the way to the
-- founding consultant. If a bad actor gets through, the chain is
-- legible.

ALTER TABLE staff
    ADD COLUMN consultation_training_completed_at TIMESTAMPTZ,
    ADD COLUMN consultation_trainer_user_id       UUID REFERENCES users(id);


-- ── social_verifications ────────────────────────────────────────────
--
-- One row per identity consultation. The consulting stylist's user_id
-- is recorded; their reputation is attached to every attestation.
-- consultation_notes are private (staff-only), can include physical
-- description or narrative details the stylist wants for their own
-- recall. consent_snapshot captures what the user consented to at
-- that moment.

CREATE TABLE social_verifications (
    id                     UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id                UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    consulted_by_user_id   UUID NOT NULL REFERENCES users(id),
    salon_id               UUID REFERENCES salons(id),
    consulted_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    consultation_notes     TEXT,
    consent_snapshot       JSONB NOT NULL DEFAULT '{}'::jsonb,
    status                 TEXT NOT NULL DEFAULT 'verified'
                           CHECK (status IN ('verified', 'withdrawn', 'expired')),
    created_at             TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_social_verifications_user       ON social_verifications(user_id);
CREATE INDEX idx_social_verifications_consultant ON social_verifications(consulted_by_user_id);

ALTER TABLE social_verifications ENABLE ROW LEVEL SECURITY;
ALTER TABLE social_verifications FORCE ROW LEVEL SECURITY;

CREATE POLICY social_verifications_self_or_admin_select ON social_verifications FOR SELECT
    USING (
        user_id = NULLIF(current_setting('app.user_id', true), '')::uuid
        OR current_setting('app.is_admin', true) = 'true'
    );
CREATE POLICY social_verifications_admin_insert ON social_verifications FOR INSERT
    WITH CHECK (current_setting('app.is_admin', true) = 'true');
CREATE POLICY social_verifications_admin_update ON social_verifications FOR UPDATE
    USING (current_setting('app.is_admin', true) = 'true')
    WITH CHECK (current_setting('app.is_admin', true) = 'true');

GRANT SELECT, INSERT, UPDATE ON social_verifications TO bf_app;


-- ── identity_cards ──────────────────────────────────────────────────
--
-- The physical artifact of a completed consultation. Serial is
-- entirely random (no salon prefix, no sequence) — enumeration
-- attacks against `/card/{serial}` are impossible.
--
-- SELECT policy is USING(true) so the public /card/{serial} lookup
-- works. The endpoint is the audit boundary — its response includes
-- only validity + issue date + design version, never user_id or PII.

CREATE TABLE identity_cards (
    id                       UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id                  UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    social_verification_id   UUID NOT NULL REFERENCES social_verifications(id),
    serial                   TEXT NOT NULL UNIQUE,
    issued_at                TIMESTAMPTZ NOT NULL DEFAULT now(),
    issued_by_user_id        UUID NOT NULL REFERENCES users(id),
    salon_id                 UUID REFERENCES salons(id),
    design_version           TEXT NOT NULL DEFAULT 'v1',
    status                   TEXT NOT NULL DEFAULT 'active'
                             CHECK (status IN ('active', 'replaced', 'revoked', 'lost')),
    replaced_by_card_id      UUID REFERENCES identity_cards(id),
    revoked_at               TIMESTAMPTZ,
    revoked_reason           TEXT,
    created_at               TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_identity_cards_user   ON identity_cards(user_id);
CREATE INDEX idx_identity_cards_status ON identity_cards(status);

ALTER TABLE identity_cards ENABLE ROW LEVEL SECURITY;
ALTER TABLE identity_cards FORCE ROW LEVEL SECURITY;

-- Wide SELECT: public /card/{serial} lookup needs to reach this table
-- before any user context exists. The endpoint sanitises the response
-- to strip PII; the application code is the audit boundary. Do not
-- add other read paths.
CREATE POLICY identity_cards_lookup ON identity_cards FOR SELECT
    USING (true);
CREATE POLICY identity_cards_admin_insert ON identity_cards FOR INSERT
    WITH CHECK (current_setting('app.is_admin', true) = 'true');
CREATE POLICY identity_cards_admin_update ON identity_cards FOR UPDATE
    USING (current_setting('app.is_admin', true) = 'true')
    WITH CHECK (current_setting('app.is_admin', true) = 'true');

GRANT SELECT, INSERT, UPDATE ON identity_cards TO bf_app;


-- ── Identity Consultation service ───────────────────────────────────

INSERT INTO services (name, description, duration_minutes, base_price_cents)
VALUES (
    'Identity Consultation',
    'Private consultation to verify identity for the Box Fraise social layer. Conducted by a trained Box Fraise consultant. No documents required.',
    60,
    0
);
