-- =============================================================
-- 0022 — Take the email back out.
--
-- 0021 made an email required so a post could be claimed later by a
-- magic link. That plan is gone: there is one way to become a member
-- of this platform and it is turning up to the run club. An emailed
-- account would have been a second, weaker identity sitting next to
-- the in-person one, which is precisely what makes "showing up means
-- something" stop being true.
--
-- So the column goes rather than lingering as a field nobody fills.
-- Nothing is lost — no submission exists in any environment — and a
-- column kept "just in case" is a column somebody eventually reads.
--
-- What replaces it: posting becomes members-only, and a member is
-- somebody an admin verified in a room. Submissions will carry a
-- user_id when that exists. Until then the feed is public to read and
-- the post button explains where memberships come from.
--
-- `submitter_name` stays. It is the name that goes on a post, and once
-- posts belong to members it becomes the member's display name.
-- =============================================================

BEGIN;

ALTER TABLE submissions DROP CONSTRAINT submissions_email_shape;
ALTER TABLE submissions DROP CONSTRAINT submissions_email_len;
ALTER TABLE submissions DROP COLUMN submitter_email;

COMMIT;
