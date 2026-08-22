-- Attention is scarce, or it is not attention.
--
-- Opening pays and refusing does not, so without a ceiling the only
-- sensible thing a reader can do is open everything. That is collection,
-- not judgement — nobody has to decide anything, because nothing is
-- given up by opening one more.
--
-- A daily limit is what turns a payment into a choice. With five a day,
-- picking which five is a real decision and the ones left unopened mean
-- something. It is the difference between being paid to look and being
-- paid to decide.
--
-- **Declining does not count.** Refusing is free and always will be —
-- charging somebody a day's attention for saying no would make silence
-- expensive, which is the opposite of the point. Only opens consume the
-- allowance.
--
-- The day is Edmonton's, not UTC's. A UTC day boundary falls at six in
-- the evening here, which would reset somebody's allowance in the middle
-- of their evening for no reason they could see.
--
-- The limit lives in its own function so changing it is one statement
-- rather than a migration, and it is enforced in the database rather
-- than the page because a limit a client can skip is not a limit.

CREATE FUNCTION bf_daily_open_limit() RETURNS INTEGER
LANGUAGE sql IMMUTABLE AS $fn$ SELECT 5 $fn$;

CREATE FUNCTION bf_opens_today(p_runner UUID) RETURNS INTEGER
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $fn$
    SELECT count(*)::integer
      FROM ad_opens
     WHERE runner_id = p_runner
       AND (opened_at AT TIME ZONE 'America/Edmonton')::date
         = (now()     AT TIME ZONE 'America/Edmonton')::date;
$fn$;

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

    -- Today's allowance. Checked before the counter moves so a refused
    -- open costs the advertiser nothing.
    IF bf_opens_today(v_runner) >= bf_daily_open_limit() THEN
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

GRANT EXECUTE ON FUNCTION bf_daily_open_limit() TO bf_app;
GRANT EXECUTE ON FUNCTION bf_opens_today(UUID) TO bf_app;

INSERT INTO schema_migrations (version) VALUES ('0044_daily_limit')
ON CONFLICT (version) DO NOTHING;
