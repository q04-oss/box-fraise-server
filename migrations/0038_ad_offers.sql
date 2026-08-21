-- The inbox: a business asks, a member decides, and the member is paid.
--
-- This is the first phone drawn on /mvp, and until now it was a CSS
-- animation with the caption "this app does not exist yet". It is the
-- only part of the platform that describes the business, so it is worth
-- being precise about what these two tables are and are not.
--
-- An OFFER is created by an admin after a business has handed over cash
-- at a run. Nothing here takes a payment: `views_paid` is how many times
-- the offer will pay out, and amount_cents * views_paid is money that
-- already changed hands in a park. There is no card network in this
-- table and there is not meant to be one — see /cash.
--
-- A VIEW is a member saying yes. It is the consent and the receipt in
-- one row: it exists only because somebody chose, and what they are owed
-- is `amount_cents` frozen at the moment they chose, not looked up
-- later from an offer an admin may since have edited.
--
-- What is deliberately NOT here:
--
--   No targeting. An offer is not addressed to anybody. Every current
--   member sees the same open offers in the same order, because the
--   whole argument of this platform is against a machine that decides
--   what you are shown based on what it knows about you.
--
--   No proof of attention. There is no dwell timer, no scroll depth, no
--   confirmation that a member looked. Saying yes IS the product being
--   sold — a person choosing to give attention to a named business — and
--   measuring whether they really looked would be surveillance bolted
--   onto the one transaction here built on consent.
--
--   No balance column. What a member is owed is a SUM over unpaid views
--   and is computed every time it is asked for. A stored balance is a
--   number that can drift from the rows that justify it, and this one
--   ends in cash being counted into somebody's hand.
--
-- Paying out is an admin marking rows paid after handing over money at a
-- run. That is the same shape as attendance: a fact recorded by somebody
-- who was standing there.

CREATE TABLE ad_offers (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- The artwork and the business both come from the mark, so an offer
    -- cannot exist for an advertisement the platform does not hold.
    mark_id      UUID NOT NULL REFERENCES marks(id) ON DELETE CASCADE,
    -- The sentence in the inbox. Short, because it sits on a phone under
    -- a business name.
    headline     TEXT NOT NULL,
    amount_cents INTEGER NOT NULL,
    -- How many times this will pay. When views_taken reaches it the
    -- offer stops appearing; the business bought a number of yeses.
    views_paid   INTEGER NOT NULL,
    views_taken  INTEGER NOT NULL DEFAULT 0,
    -- Some of the advertisements are nudes and most are not. The flag is
    -- what lets a surface that must not show them — an iOS app, if one
    -- is ever built — ask for the rest.
    explicit     BOOLEAN NOT NULL DEFAULT false,
    status       TEXT NOT NULL DEFAULT 'open',
    created_by_admin_id UUID REFERENCES admins(id),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT ad_offers_status_known CHECK (status IN ('open', 'closed')),
    -- A ceiling of $100 a view. Not a policy about what an advertisement
    -- is worth — a guard against a typo becoming a debt.
    CONSTRAINT ad_offers_amount_sane CHECK (amount_cents > 0 AND amount_cents <= 10000),
    CONSTRAINT ad_offers_views_sane
        CHECK (views_paid > 0 AND views_taken >= 0 AND views_taken <= views_paid),
    CONSTRAINT ad_offers_headline_sane CHECK (length(headline) BETWEEN 1 AND 140)
);

CREATE INDEX idx_ad_offers_open ON ad_offers (created_at DESC)
    WHERE status = 'open';

CREATE TABLE ad_views (
    id           UUID PRIMARY KEY,
    offer_id     UUID NOT NULL REFERENCES ad_offers(id) ON DELETE CASCADE,
    user_id      UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- Frozen at the moment of consent. An admin editing the offer later
    -- must not change what somebody is already owed.
    amount_cents INTEGER NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    paid_at      TIMESTAMPTZ,
    paid_by_admin_id UUID REFERENCES admins(id),
    -- One member, one yes, per offer. Also the race-close: two taps
    -- arriving together cannot both take a view off the budget.
    CONSTRAINT ad_views_once UNIQUE (offer_id, user_id)
);

CREATE INDEX idx_ad_views_unpaid ON ad_views (user_id) WHERE paid_at IS NULL;

ALTER TABLE ad_offers ENABLE ROW LEVEL SECURITY;
ALTER TABLE ad_offers FORCE ROW LEVEL SECURITY;
ALTER TABLE ad_views  ENABLE ROW LEVEL SECURITY;
ALTER TABLE ad_views  FORCE ROW LEVEL SECURITY;

-- Members see open offers; nobody signed out sees any. This is not a
-- public list: what a business is paying, and how much attention is
-- still unbought, is between the business and the platform.
CREATE POLICY ad_offers_member_select ON ad_offers FOR SELECT
    USING (
        (status = 'open'
         AND NULLIF(current_setting('app.user_id', true), '') IS NOT NULL)
        OR current_setting('app.is_admin', true) = 'true'
    );
CREATE POLICY ad_offers_admin_insert ON ad_offers FOR INSERT
    WITH CHECK (current_setting('app.is_admin', true) = 'true');
CREATE POLICY ad_offers_admin_update ON ad_offers FOR UPDATE
    USING (current_setting('app.is_admin', true) = 'true');
CREATE POLICY ad_offers_admin_delete ON ad_offers FOR DELETE
    USING (current_setting('app.is_admin', true) = 'true');

-- A member reads their own receipts and nobody else's.
CREATE POLICY ad_views_own_select ON ad_views FOR SELECT
    USING (
        user_id = NULLIF(current_setting('app.user_id', true), '')::uuid
        OR current_setting('app.is_admin', true) = 'true'
    );
-- Only to mark money handed over. There is no member UPDATE and there
-- should not be: what you were owed and whether you were paid are both
-- facts about a moment somebody else was present for.
CREATE POLICY ad_views_admin_update ON ad_views FOR UPDATE
    USING (current_setting('app.is_admin', true) = 'true');

-- Accepting is a SECURITY DEFINER function and `bf_app` gets no INSERT
-- on ad_views at all, which is tighter than a policy would be: the app
-- can take one offer as the member the transaction is already scoped
-- to, and cannot write an arbitrary receipt.
--
-- Three things have to happen together or not at all — the budget is
-- checked, the counter moves, and the receipt is written — and the
-- member has no UPDATE on ad_offers to move the counter with.
--
-- The user is read from the GUC rather than taken as an argument. A
-- caller cannot accept on somebody else's behalf because there is no
-- parameter with which to try.
--
-- `UPDATE ... WHERE status = 'open' AND views_taken < views_paid` is the
-- race-close, the same idiom as the verify flip in 0001: two members
-- taking the last view of an offer at the same instant means exactly one
-- UPDATE returns a row.
CREATE FUNCTION bf_accept_offer(p_offer UUID, p_id UUID) RETURNS INTEGER
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $fn$
DECLARE
    v_user   UUID := NULLIF(current_setting('app.user_id', true), '')::uuid;
    v_amount INTEGER;
BEGIN
    IF v_user IS NULL THEN
        RETURN NULL;
    END IF;
    -- Posting lapses when somebody stops turning up, and so does this.
    -- Being paid by the platform is a thing a member does, and a
    -- membership is kept rather than got.
    IF NOT bf_member_is_current(v_user) THEN
        RETURN NULL;
    END IF;

    UPDATE ad_offers
       SET views_taken = views_taken + 1
     WHERE id = p_offer
       AND status = 'open'
       AND views_taken < views_paid
    RETURNING amount_cents INTO v_amount;

    IF v_amount IS NULL THEN
        RETURN NULL;
    END IF;

    INSERT INTO ad_views (id, offer_id, user_id, amount_cents)
    VALUES (p_id, p_offer, v_user, v_amount);

    RETURN v_amount;
EXCEPTION
    -- Already said yes to this one. The nested block means the counter
    -- bump above rolls back with it, so a second tap costs the business
    -- nothing.
    WHEN unique_violation THEN
        RETURN NULL;
END;
$fn$;

GRANT SELECT, INSERT, UPDATE, DELETE ON ad_offers TO bf_app;
GRANT SELECT, UPDATE ON ad_views TO bf_app;
GRANT EXECUTE ON FUNCTION bf_accept_offer(UUID, UUID) TO bf_app;

INSERT INTO schema_migrations (version) VALUES ('0038_ad_offers')
ON CONFLICT (version) DO NOTHING;
