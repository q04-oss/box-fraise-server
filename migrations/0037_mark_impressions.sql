-- How many times a billboard was actually run past.
--
-- The number a business asks for before it pays, and the one Box Fraise
-- has never been able to give. Attendance answers "how many people";
-- this answers "how many times was the thing I bought in front of
-- somebody".
--
-- Daily counters, not an event log. One row per mark per day, bumped on
-- conflict — so the table holds "this billboard was seen 340 times on
-- the 12th" and can never hold "somebody saw it at 14:22". An
-- impressions table with a row per view is a record of when individual
-- people were playing a game, which is worth nothing to an advertiser
-- and is exactly the kind of thing this project argues against
-- everywhere else. The aggregate is the product; the events are not
-- collected at all.
--
-- The app never writes this table directly. Bumping a daily counter
-- means INSERT ... ON CONFLICT DO UPDATE, and that has to see the
-- conflicting row to update it — which a public transaction cannot,
-- because SELECT here is admin-only and should stay that way: a
-- business's figures are not published on the site. So the write goes
-- through a SECURITY DEFINER function, the same shape as
-- bf_pending_submission_count in 0018. bf_app gets EXECUTE and no
-- INSERT or UPDATE at all, which is tighter than the policy would have
-- been: it can add to a counter and cannot write an arbitrary row.
--
-- Nothing here identifies anybody. There is no user, no session, no
-- address, no device — a mark, a date and an integer.
--
-- The count is reported by the browser and can be inflated by anybody
-- who wants to, the same as the leaderboard. The cap in the service is
-- for garbage. The real answer is that a number a business does not
-- believe is worth nothing to sell, and the platform's whole argument
-- is that it does not need to lie about attention.

CREATE TABLE mark_impressions (
    mark_id UUID NOT NULL REFERENCES marks(id) ON DELETE CASCADE,
    day     DATE NOT NULL DEFAULT CURRENT_DATE,
    seen    INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (mark_id, day),
    CONSTRAINT mark_impressions_sane CHECK (seen >= 0)
);

CREATE INDEX idx_mark_impressions_day ON mark_impressions (day DESC);

ALTER TABLE mark_impressions ENABLE ROW LEVEL SECURITY;
ALTER TABLE mark_impressions FORCE ROW LEVEL SECURITY;

-- Only an admin reads it. A business is told its figure; it is not
-- published on the site, and no policy lets the app write directly.
CREATE POLICY mark_impressions_admin_select ON mark_impressions FOR SELECT
    USING (current_setting('app.is_admin', true) = 'true');

CREATE FUNCTION bf_record_impression(p_mark UUID, p_seen INTEGER) RETURNS VOID
    LANGUAGE sql SECURITY DEFINER SET search_path = public AS $fn$
    INSERT INTO mark_impressions (mark_id, seen)
    VALUES (p_mark, p_seen)
    ON CONFLICT (mark_id, day)
    DO UPDATE SET seen = mark_impressions.seen + p_seen;
$fn$;

GRANT SELECT ON mark_impressions TO bf_app;
GRANT EXECUTE ON FUNCTION bf_record_impression(UUID, INTEGER) TO bf_app;

INSERT INTO schema_migrations (version) VALUES ('0037_mark_impressions')
ON CONFLICT (version) DO NOTHING;
