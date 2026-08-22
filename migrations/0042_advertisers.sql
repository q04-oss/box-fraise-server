-- Advertisers, so that spend can be totalled.
--
-- `adverts.advertiser` was free text. Two adverts from the same bakery
-- were two unrelated strings in a column, so nothing could answer "what
-- has Ferngrove spent here" — not for them and not for the platform
-- either. That is a data-model gap rather than a missing screen, and
-- this closes it.
--
-- The text column stays. It is the name as it appeared in the inbox at
-- the time, and a business renaming itself should not rewrite what its
-- old adverts said. `advertiser_id` is the thing that joins them.
--
-- `paid_at` is the other half. Until now nothing recorded that money had
-- arrived — an advert going up was the only evidence, and evidence that
-- lives in somebody's memory is not a ledger. It is set at the moment a
-- request is accepted, which is meant to be the moment the invoice was
-- settled.
--
-- **The ledger is read by a capability, not an account.** A business
-- does not sign in; they are sent an unguessable address. There is
-- therefore no public SELECT policy on either table — the page reads
-- through a SECURITY DEFINER function that takes exactly one advertiser
-- id and can return nothing else. Knowing the id is the permission, and
-- there is no query shape that walks from one to another.

CREATE TABLE advertisers (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name       TEXT NOT NULL,
    -- Nullable: an advertiser created by backfill has no contact, and a
    -- business reached by telephone may never give one.
    contact    TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT advertisers_name_sane CHECK (length(name) BETWEEN 1 AND 80)
);

-- Matching is by lowercased name, so two adverts typed slightly
-- differently still land on one business.
CREATE UNIQUE INDEX idx_advertisers_name ON advertisers (lower(name));

ALTER TABLE adverts ADD COLUMN advertiser_id UUID REFERENCES advertisers(id);
ALTER TABLE adverts ADD COLUMN paid_at TIMESTAMPTZ;

-- Backfill, so this migration is correct whether or not adverts already
-- exist. One advertiser per distinct name; anything already up is
-- treated as paid for, because it would not have been put up otherwise.
INSERT INTO advertisers (name)
SELECT DISTINCT advertiser FROM adverts
ON CONFLICT DO NOTHING;

UPDATE adverts a
   SET advertiser_id = v.id,
       paid_at = COALESCE(a.paid_at, a.created_at)
  FROM advertisers v
 WHERE lower(v.name) = lower(a.advertiser)
   AND a.advertiser_id IS NULL;

ALTER TABLE adverts ALTER COLUMN advertiser_id SET NOT NULL;

CREATE INDEX idx_adverts_advertiser ON adverts (advertiser_id, created_at DESC);

ALTER TABLE advertisers ENABLE ROW LEVEL SECURITY;
ALTER TABLE advertisers FORCE  ROW LEVEL SECURITY;

-- Admin only. The ledger page does not read this table directly; see
-- the function below.
CREATE POLICY advertisers_admin_select ON advertisers FOR SELECT
    USING (current_setting('app.is_admin', true) = 'true');
CREATE POLICY advertisers_admin_insert ON advertisers FOR INSERT
    WITH CHECK (current_setting('app.is_admin', true) = 'true');
CREATE POLICY advertisers_admin_update ON advertisers FOR UPDATE
    USING (current_setting('app.is_admin', true) = 'true');

-- One advertiser's ledger, by id, and nothing else.
--
-- SECURITY DEFINER because the page that calls it has no session at all
-- — a business is sent a link, not an account. The parameter is the
-- whole permission: there is no way to ask for a list, no way to walk
-- from one advertiser to another, and no way to reach an advert that
-- belongs to somebody else.
--
-- `body` is deliberately absent. What an advert says when opened is not
-- part of what it cost, and this function is read by a page on the open
-- internet.
CREATE FUNCTION bf_advertiser_ledger(p_advertiser UUID)
RETURNS TABLE (
    name        TEXT,
    advert_id   UUID,
    teaser      TEXT,
    price_cents INTEGER,
    pays_cents  INTEGER,
    opens_paid  INTEGER,
    opens_taken INTEGER,
    status      TEXT,
    paid_at     TIMESTAMPTZ,
    created_at  TIMESTAMPTZ
)
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $fn$
    SELECT v.name, a.id, a.teaser, a.price_cents, a.pays_cents,
           a.opens_paid, a.opens_taken, a.status, a.paid_at, a.created_at
      FROM adverts a
      JOIN advertisers v ON v.id = a.advertiser_id
     WHERE a.advertiser_id = p_advertiser
     ORDER BY a.created_at DESC;
$fn$;

-- Find a business by name or make one. Used when an advert is created,
-- so that two adverts typed months apart still total together.
CREATE FUNCTION bf_advertiser_for(p_name TEXT, p_contact TEXT) RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $fn$
DECLARE
    v_id UUID;
BEGIN
    SELECT id INTO v_id FROM advertisers WHERE lower(name) = lower(trim(p_name));
    IF v_id IS NOT NULL THEN
        -- A later request carrying a contact fills in one we never had.
        UPDATE advertisers
           SET contact = COALESCE(NULLIF(trim(p_contact), ''), contact)
         WHERE id = v_id;
        RETURN v_id;
    END IF;
    INSERT INTO advertisers (name, contact)
    VALUES (trim(p_name), NULLIF(trim(p_contact), ''))
    RETURNING id INTO v_id;
    RETURN v_id;
END;
$fn$;

GRANT SELECT, INSERT, UPDATE ON advertisers TO bf_app;
GRANT EXECUTE ON FUNCTION bf_advertiser_ledger(UUID) TO bf_app;
GRANT EXECUTE ON FUNCTION bf_advertiser_for(TEXT, TEXT) TO bf_app;

INSERT INTO schema_migrations (version) VALUES ('0042_advertisers')
ON CONFLICT (version) DO NOTHING;
