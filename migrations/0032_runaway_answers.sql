-- Anonymous answers to "why do you want to run away?"
--
-- This is the only unauthenticated write on the platform. 0023 closed
-- the last one when posting became members-only, and /onboard/register
-- is a device path rather than a public form. So everything here exists
-- to bound one open door.
--
-- Deliberately NOT a submission. A submission carries a member number,
-- which is the whole identity system: a byline that means somebody
-- turned up. Letting anonymous rows into that table would need a
-- nullable member_no and would put unattributed text in the feed beside
-- people who walked to a park at eight in the morning. Both the feed
-- and the number would stop meaning anything.
--
-- So this is a separate table, read on /runaway and nowhere else. The
-- upgrade path is the point: anonymous gets you read by an editor, a
-- number gets you published in the magazine.
--
-- Text only. No title, no photograph, no contact. An image endpoint on
-- an anonymous write path is a different and much worse problem.

CREATE TABLE runaway_answers (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    body        TEXT NOT NULL,
    status      TEXT NOT NULL DEFAULT 'pending',
    submitted_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    reviewed_at  TIMESTAMPTZ,
    reviewed_by_admin_id UUID REFERENCES admins(id),
    CONSTRAINT runaway_answers_status_known CHECK (status IN ('pending', 'accepted')),
    -- Long enough to be an answer, short enough that the queue stays
    -- readable and a script cannot fill the table with novels.
    CONSTRAINT runaway_answers_length
        CHECK (char_length(body) BETWEEN 20 AND 2000)
);

CREATE INDEX idx_runaway_answers_accepted
    ON runaway_answers (reviewed_at DESC) WHERE status = 'accepted';
CREATE INDEX idx_runaway_answers_pending
    ON runaway_answers (submitted_at ASC) WHERE status = 'pending';

ALTER TABLE runaway_answers ENABLE ROW LEVEL SECURITY;
ALTER TABLE runaway_answers FORCE ROW LEVEL SECURITY;

-- Anyone may write, but only ever as 'pending'. The status is hardcoded
-- in the repository as well; this is the second of the two barriers,
-- and the one that holds if the first is ever edited carelessly.
CREATE POLICY runaway_answers_public_insert ON runaway_answers FOR INSERT
    WITH CHECK (status = 'pending');

-- And read only what an editor has accepted. A pending answer is
-- invisible to everybody but an admin, which is what stops this being a
-- public wall anybody can write on.
CREATE POLICY runaway_answers_public_select ON runaway_answers FOR SELECT
    USING (status = 'accepted' OR current_setting('app.is_admin', true) = 'true');

CREATE POLICY runaway_answers_admin_update ON runaway_answers FOR UPDATE
    USING (current_setting('app.is_admin', true) = 'true');
CREATE POLICY runaway_answers_admin_delete ON runaway_answers FOR DELETE
    USING (current_setting('app.is_admin', true) = 'true');

-- Counting pending rows is how the door closes under load, and a public
-- transaction cannot see them to count them. Same shape as
-- bf_pending_submission_count in 0018.
CREATE FUNCTION bf_pending_runaway_count() RETURNS INTEGER
    LANGUAGE sql SECURITY DEFINER SET search_path = public AS $$
    SELECT COUNT(*)::int FROM runaway_answers WHERE status = 'pending';
$$;

GRANT SELECT, INSERT, UPDATE, DELETE ON runaway_answers TO bf_app;
GRANT EXECUTE ON FUNCTION bf_pending_runaway_count() TO bf_app;

INSERT INTO schema_migrations (version) VALUES ('0032_runaway_answers')
ON CONFLICT (version) DO NOTHING;
