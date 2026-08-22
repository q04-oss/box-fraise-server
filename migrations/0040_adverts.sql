-- The inbox: advertisements a signed-in runner chooses to open, and is
-- paid for opening.
--
-- Standalone. This does not touch `users`, `marks`, `businesses`,
-- `ad_offers` or anything else from the earlier platform — it belongs to
-- the running layer and is scoped to `runners` (0039) and nothing else.
--
-- An advert has two halves. `teaser` is what sits in the inbox unopened:
-- who is asking and what it is worth. `body` is what appears when
-- somebody chooses to open it, and is not readable until they do — the
-- policy below is what enforces that, not the page.
--
-- Nothing here charges anybody. `pays_cents * opens_paid` is money an
-- advertiser handed over before the row existed; this records what it
-- bought. What a runner is owed is a SUM over unpaid opens, computed
-- every time, because a stored balance can drift from the rows that
-- justify it.
--
-- There is no targeting. Every signed-in runner sees the same open
-- adverts in the same order, and the only thing that removes one from
-- somebody's list is that they already opened it or the budget ran out.
-- Nothing measures whether they read it. Choosing to open it is the
-- whole transaction.

CREATE TABLE adverts (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- Who is asking. Shown unopened.
    advertiser   TEXT NOT NULL,
    -- The one line shown unopened, next to the amount.
    teaser       TEXT NOT NULL,
    -- What opening it reveals. Never sent to anybody who has not opened.
    body         TEXT NOT NULL,
    -- Optional somewhere to go once it is open.
    link         TEXT,
    pays_cents   INTEGER NOT NULL,
    opens_paid   INTEGER NOT NULL,
    opens_taken  INTEGER NOT NULL DEFAULT 0,
    status       TEXT NOT NULL DEFAULT 'open',
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT adverts_status_known CHECK (status IN ('open', 'closed')),
    -- A ceiling of $100 an open. Not a view about what attention is
    -- worth — a guard against a typo becoming a debt.
    CONSTRAINT adverts_pays_sane CHECK (pays_cents > 0 AND pays_cents <= 10000),
    CONSTRAINT adverts_opens_sane
        CHECK (opens_paid > 0 AND opens_taken >= 0 AND opens_taken <= opens_paid),
    CONSTRAINT adverts_text_sane CHECK (
        length(advertiser) BETWEEN 1 AND 80 AND
        length(teaser)     BETWEEN 1 AND 140 AND
        length(body)       BETWEEN 1 AND 4000
    )
);

CREATE INDEX idx_adverts_open ON adverts (created_at) WHERE status = 'open';

CREATE TABLE ad_opens (
    id           UUID PRIMARY KEY,
    advert_id    UUID NOT NULL REFERENCES adverts(id) ON DELETE CASCADE,
    runner_id    UUID NOT NULL REFERENCES runners(id) ON DELETE CASCADE,
    -- Frozen at the moment of choosing. An advertiser editing the advert
    -- afterwards must not change what somebody is already owed.
    amount_cents INTEGER NOT NULL,
    opened_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    paid_at      TIMESTAMPTZ,
    -- One runner, one open, per advert. Also the race-close: two taps
    -- arriving together cannot both take an open off the budget.
    CONSTRAINT ad_opens_once UNIQUE (advert_id, runner_id)
);

CREATE INDEX idx_ad_opens_unpaid ON ad_opens (runner_id) WHERE paid_at IS NULL;

ALTER TABLE adverts  ENABLE ROW LEVEL SECURITY;
ALTER TABLE adverts  FORCE  ROW LEVEL SECURITY;
ALTER TABLE ad_opens ENABLE ROW LEVEL SECURITY;
ALTER TABLE ad_opens FORCE  ROW LEVEL SECURITY;

-- Signed-in runners see open adverts; nobody signed out sees any. What
-- an advertiser is paying, and how much of it is unspent, is between
-- them and the platform.
--
-- Row-level security cannot hide a column, so `body` is reachable by any
-- query this policy admits. The boundary is the repository: the listing
-- names its columns and never selects `body`, and the one statement that
-- does is the one that has just written an open. Never `SELECT *` here.
CREATE POLICY adverts_runner_select ON adverts FOR SELECT
    USING (
        (status = 'open'
         AND NULLIF(current_setting('app.runner_id', true), '') IS NOT NULL)
        OR current_setting('app.is_admin', true) = 'true'
    );
CREATE POLICY adverts_admin_insert ON adverts FOR INSERT
    WITH CHECK (current_setting('app.is_admin', true) = 'true');
CREATE POLICY adverts_admin_update ON adverts FOR UPDATE
    USING (current_setting('app.is_admin', true) = 'true');
CREATE POLICY adverts_admin_delete ON adverts FOR DELETE
    USING (current_setting('app.is_admin', true) = 'true');

-- A runner reads their own receipts and nobody else's.
CREATE POLICY ad_opens_own_select ON ad_opens FOR SELECT
    USING (
        runner_id = NULLIF(current_setting('app.runner_id', true), '')::uuid
        OR current_setting('app.is_admin', true) = 'true'
    );
-- Only to mark money handed over. There is no runner UPDATE.
CREATE POLICY ad_opens_admin_update ON ad_opens FOR UPDATE
    USING (current_setting('app.is_admin', true) = 'true');

-- Opening is a SECURITY DEFINER function and `bf_app` gets no INSERT on
-- ad_opens at all, which is tighter than a policy: the app can open one
-- advert as the runner the transaction is already scoped to, and cannot
-- write an arbitrary receipt.
--
-- Three things have to happen together — the budget is checked, the
-- counter moves, the receipt is written — and a runner has no UPDATE on
-- adverts to move the counter with.
--
-- The runner is read from the GUC rather than taken as an argument, so
-- there is no parameter with which to open something on somebody else's
-- behalf. `UPDATE ... WHERE status = 'open' AND opens_taken < opens_paid`
-- is the race-close: two runners taking the last open of an advert at
-- the same instant means exactly one UPDATE returns a row.
CREATE FUNCTION bf_open_advert(p_advert UUID, p_id UUID) RETURNS INTEGER
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $fn$
DECLARE
    v_runner UUID := NULLIF(current_setting('app.runner_id', true), '')::uuid;
    v_amount INTEGER;
BEGIN
    IF v_runner IS NULL THEN
        RETURN NULL;
    END IF;

    UPDATE adverts
       SET opens_taken = opens_taken + 1
     WHERE id = p_advert
       AND status = 'open'
       AND opens_taken < opens_paid
    RETURNING pays_cents INTO v_amount;

    IF v_amount IS NULL THEN
        RETURN NULL;
    END IF;

    INSERT INTO ad_opens (id, advert_id, runner_id, amount_cents)
    VALUES (p_id, p_advert, v_runner, v_amount);

    RETURN v_amount;
EXCEPTION
    -- Already opened this one. The nested block means the counter bump
    -- rolls back with it, so a second tap costs the advertiser nothing.
    WHEN unique_violation THEN
        RETURN NULL;
END;
$fn$;

GRANT SELECT, INSERT, UPDATE, DELETE ON adverts  TO bf_app;
GRANT SELECT, UPDATE                 ON ad_opens TO bf_app;
GRANT EXECUTE ON FUNCTION bf_open_advert(UUID, UUID) TO bf_app;

INSERT INTO schema_migrations (version) VALUES ('0040_adverts')
ON CONFLICT (version) DO NOTHING;
