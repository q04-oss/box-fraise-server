-- Saying no.
--
-- The inbox could be opened or ignored, and ignoring is not refusing —
-- an advert nobody opens sits there forever, which is a delay dressed up
-- as a choice. This is the other half: a decision that is recorded,
-- honoured, and costs the advertiser nothing.
--
-- A separate table from `ad_opens` on purpose. That one is a receipt: it
-- exists because money is owed, and every column on it is about the
-- money. This one has no money in it at all. Putting a declined row in
-- the receipts table with a zero amount would mean every balance query
-- had to remember to exclude it, and one day one of them would not.
--
-- What a decline does:
--   * it leaves that runner's inbox for good
--   * nobody is paid
--   * `opens_taken` does not move, so the advertiser is not charged —
--     they buy opens, and this was not one
--
-- What it is worth to the advertiser is the count. "Forty people were
-- shown the line, three opened it, nine said no" is the most honest
-- figure this platform can produce about an advertisement, and it is
-- only possible because refusing is an act rather than an absence.
--
-- No SECURITY DEFINER function here, unlike opening. Opening has to move
-- a counter on a row the runner cannot write, so it needs one. Declining
-- writes a single row the runner owns, which an ordinary policy covers.

CREATE TABLE ad_declines (
    id          UUID PRIMARY KEY,
    advert_id   UUID NOT NULL REFERENCES adverts(id) ON DELETE CASCADE,
    runner_id   UUID NOT NULL REFERENCES runners(id) ON DELETE CASCADE,
    declined_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- One decision per runner per advert, the same rule opening has.
    CONSTRAINT ad_declines_once UNIQUE (advert_id, runner_id)
);

CREATE INDEX idx_ad_declines_advert ON ad_declines (advert_id);

ALTER TABLE ad_declines ENABLE ROW LEVEL SECURITY;
ALTER TABLE ad_declines FORCE  ROW LEVEL SECURITY;

CREATE POLICY ad_declines_own_select ON ad_declines FOR SELECT
    USING (
        runner_id = NULLIF(current_setting('app.runner_id', true), '')::uuid
        OR current_setting('app.is_admin', true) = 'true'
    );
CREATE POLICY ad_declines_own_insert ON ad_declines FOR INSERT
    WITH CHECK (runner_id = NULLIF(current_setting('app.runner_id', true), '')::uuid);

GRANT SELECT, INSERT ON ad_declines TO bf_app;

-- Opening must now also refuse an advert that was already said no to.
-- The listing excludes declined ones so there is no button for it, but
-- the rule belongs here rather than in a page.
CREATE OR REPLACE FUNCTION bf_open_advert(p_advert UUID, p_id UUID) RETURNS INTEGER
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

    -- A no is a no, including to yourself later.
    IF EXISTS (SELECT 1 FROM ad_declines
                WHERE advert_id = p_advert AND runner_id = v_runner) THEN
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
    WHEN unique_violation THEN
        RETURN NULL;
END;
$fn$;

-- The ledger gains the count of people who said no. Dropped and
-- recreated rather than replaced because the returned columns change.
DROP FUNCTION IF EXISTS bf_advertiser_ledger(UUID);

CREATE FUNCTION bf_advertiser_ledger(p_advertiser UUID)
RETURNS TABLE (
    name          TEXT,
    advert_id     UUID,
    teaser        TEXT,
    price_cents   INTEGER,
    pays_cents    INTEGER,
    opens_paid    INTEGER,
    opens_taken   INTEGER,
    declines      BIGINT,
    status        TEXT,
    paid_at       TIMESTAMPTZ,
    created_at    TIMESTAMPTZ
)
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $fn$
    SELECT v.name, a.id, a.teaser, a.price_cents, a.pays_cents,
           a.opens_paid, a.opens_taken,
           (SELECT count(*) FROM ad_declines d WHERE d.advert_id = a.id)::bigint,
           a.status, a.paid_at, a.created_at
      FROM adverts a
      JOIN advertisers v ON v.id = a.advertiser_id
     WHERE a.advertiser_id = p_advertiser
     ORDER BY a.created_at DESC;
$fn$;

GRANT EXECUTE ON FUNCTION bf_advertiser_ledger(UUID) TO bf_app;

INSERT INTO schema_migrations (version) VALUES ('0043_ad_declines')
ON CONFLICT (version) DO NOTHING;
