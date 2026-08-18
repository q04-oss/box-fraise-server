-- =============================================================
-- 0023 — A post belongs to a member.
--
-- Posting stops being open. There is one way to become a member of
-- this platform — turn up to the run club and have an admin make you
-- an account, in person — and from here the database enforces it
-- rather than the interface merely declining to offer a form.
--
-- The INSERT policy changes from "anybody, as pending" to "the member
-- the row says it belongs to, as pending". A request with no
-- app.user_id set cannot insert at all: `NULL = anything` is NULL, not
-- true, so the WITH CHECK fails. That is the whole enforcement, and it
-- holds even if a handler forgets to check.
--
-- `submitter_name` stays denormalised rather than joining users for a
-- display name. Under RLS an inner join is also a filter: the feed is
-- read with no user context, `users_self_or_admin_select` would match
-- nothing, and every post would silently vanish from the feed. Copying
-- the name at post time keeps the public read free of the users table
-- entirely. See CLAUDE.md.
--
-- ON DELETE CASCADE: a post is a membership act, so removing a member
-- removes their posts. If published work should ever outlive the
-- account that made it, this is the line to revisit — and it needs a
-- nullable column, not a different cascade.
-- =============================================================

BEGIN;

-- No rows exist anywhere, so this needs no default and no backfill.
ALTER TABLE submissions
    ADD COLUMN user_id UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE;

CREATE INDEX idx_submissions_user ON submissions (user_id, submitted_at DESC);

-- The old door is closed.
DROP POLICY submissions_public_insert ON submissions;

-- Members only, still only as 'pending'. Both halves are load-bearing:
-- the status clause stops a member publishing themselves, and the
-- user_id clause stops one member posting as another.
CREATE POLICY submissions_member_insert ON submissions
    FOR INSERT
    WITH CHECK (
        status = 'pending'
        AND user_id = NULLIF(current_setting('app.user_id', true), '')::uuid
    );

-- A member may read their own submissions in any state, so they can
-- see whether something they sent is still waiting. Accepted rows stay
-- public to everyone via submissions_public_select_accepted (0020).
CREATE POLICY submissions_own_select ON submissions
    FOR SELECT
    USING (user_id = NULLIF(current_setting('app.user_id', true), '')::uuid);

COMMIT;
