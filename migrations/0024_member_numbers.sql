-- =============================================================
-- 0024 — Members have a number, not a name.
--
-- Nothing public about a member is chosen by them. A number cannot be
-- squatted, cannot impersonate anybody, needs no moderation, and never
-- has to be checked against somebody else's. It also removes the one
-- field on this platform a person could have used to be somebody in
-- particular before turning up.
--
-- It is sequential, in the order people joined. That is a fact about
-- when you showed up rather than a score, which is the same line the
-- rest of the project holds: numbers describe places and moments, not
-- people's standing.
--
-- `display_name` stays, and is now used by exactly one thing: pairings,
-- where two people who already met in person and both said yes can put
-- a name to each other. That is a private channel after consent, not a
-- public identity, and a number would be useless there.
--
-- On submissions the number is denormalised, for the same reason the
-- name was: the feed is read with no user context, and under RLS a
-- join to users would match nothing and drop every post.
-- =============================================================

BEGIN;

CREATE SEQUENCE member_no_seq AS INTEGER START 1;

-- Nullable: a row registered through the iOS path is a pending user
-- and not a member yet. A number means somebody verified you.
ALTER TABLE users ADD COLUMN member_no INTEGER UNIQUE;

-- Anybody already verified gets one, oldest first, so the ordering
-- means what it says from the beginning.
WITH ordered AS (
    SELECT id, row_number() OVER (ORDER BY verified_at, id) AS n
      FROM users
     WHERE status = 'verified'
)
UPDATE users u
   SET member_no = ordered.n
  FROM ordered
 WHERE u.id = ordered.id;

SELECT setval('member_no_seq', COALESCE((SELECT MAX(member_no) FROM users), 0) + 1, false);

ALTER TABLE users ADD CONSTRAINT users_member_no_positive
    CHECK (member_no IS NULL OR member_no > 0);

-- A verified user must have one. Pending users must not.
ALTER TABLE users ADD CONSTRAINT users_member_no_matches_status
    CHECK (
        (status = 'verified' AND member_no IS NOT NULL)
        OR (status = 'pending' AND member_no IS NULL)
    );

-- ── The byline on a post ───────────────────────────────────────────
ALTER TABLE submissions DROP CONSTRAINT submissions_name_len;
ALTER TABLE submissions DROP COLUMN submitter_name;
ALTER TABLE submissions ADD COLUMN member_no INTEGER NOT NULL;

GRANT USAGE, SELECT ON SEQUENCE member_no_seq TO bf_app;

COMMIT;
