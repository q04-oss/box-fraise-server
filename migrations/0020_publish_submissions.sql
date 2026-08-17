-- =============================================================
-- 0020 — Submissions are posts, not letters.
--
-- 0018 built this table as private correspondence to an editor: no
-- public SELECT policy at all, in any state. That was the wrong model.
-- What gurgle collects is a social platform's posts — people write
-- them expecting to be read — and the magazine is a selection an
-- editor makes from what is already public.
--
-- So an accepted submission becomes readable by anyone. 'pending'
-- still is not: an open write path with no moderation in front of it
-- would publish whatever anybody typed, and the queue is what stops
-- that. The editor's Accept is now an act of publication.
--
-- What stays private is `submitter_contact`. A name goes on a post
-- because the writer put it there; a way to reach them is for the
-- editor alone. RLS is row-level, so this policy does expose that
-- column to any query the app makes against an accepted row — the
-- boundary is the repository, which selects columns explicitly and
-- never reads contact on a public path. If you add another public
-- read here, select the columns by name.
-- =============================================================

BEGIN;

CREATE POLICY submissions_public_select_accepted ON submissions
    FOR SELECT
    USING (status = 'accepted');

COMMENT ON COLUMN submissions.submitter_contact IS
    'Editor only. Never returned on a public read path — see 0020.';

COMMIT;
