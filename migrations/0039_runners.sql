-- The running layer: anybody can sign up, log runs, and be on a board.
--
-- **A runner is not a member.** This is the important thing about this
-- migration and the reason it introduces a whole parallel population
-- rather than adding columns to `users`.
--
-- A member is a number, granted in person at a run by an admin who was
-- looking at them, and it cannot be got any other way. That rule is what
-- makes a member's byline mean something and it is not being relaxed. A
-- runner is the opposite by design: a username, a password, and no
-- verification at all. Anyone on the internet may become one.
--
-- Keeping them in separate tables with separate credentials and a
-- separate GUC means the two can never be confused by a policy. A
-- runner's session cannot satisfy anything that asks for `app.user_id`,
-- and a member's cannot satisfy `app.runner_id`. Somebody may of course
-- be both; they are simply two facts about the same person, which is
-- exactly what they are in real life — one is "I ran", the other is "I
-- turned up".
--
-- What is stored about a run is deliberately thin: **how far, how long,
-- and when it started.** No route, no coordinates, no trail. The
-- browser watches its own position to work out a distance and sends the
-- total; the path is never transmitted and there is nowhere here to put
-- it. See src/domain/runs/live.rs for the same rule applied to live
-- witnessing — a run can be watched as it happens and leaves nothing
-- behind afterwards.

CREATE TABLE runners (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- Lowercased on the way in so that two people cannot take the same
    -- name in different cases. It is the byline on the board.
    username   TEXT NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT runners_username_shape
        CHECK (username ~ '^[a-z0-9_]{3,20}$')
);

-- Split from `runners` so the board can read names without any policy
-- ever putting a password hash in reach of a public query. RLS is
-- row-level, not column-level; separating the table is the only way to
-- be certain.
CREATE TABLE runner_credentials (
    runner_id     UUID PRIMARY KEY REFERENCES runners(id) ON DELETE CASCADE,
    password_hash TEXT NOT NULL
);

CREATE TABLE runner_sessions (
    token_hash TEXT PRIMARY KEY,
    runner_id  UUID NOT NULL REFERENCES runners(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Unlike a member's session, this one expires. A member who lost
    -- their credential has to turn up to a run to get another; a runner
    -- types their password again. There is no lockout to protect them
    -- from, so the token can be short-lived.
    expires_at TIMESTAMPTZ NOT NULL DEFAULT now() + interval '90 days'
);

CREATE INDEX idx_runner_sessions_runner ON runner_sessions (runner_id);

CREATE TABLE logged_runs (
    id          UUID PRIMARY KEY,
    runner_id   UUID NOT NULL REFERENCES runners(id) ON DELETE CASCADE,
    distance_m  INTEGER NOT NULL,
    duration_s  INTEGER NOT NULL,
    started_at  TIMESTAMPTZ NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT logged_runs_distance_sane
        CHECK (distance_m > 100 AND distance_m <= 200000),
    CONSTRAINT logged_runs_duration_sane
        CHECK (duration_s >= 60 AND duration_s <= 86400),
    -- Nobody averages faster than 8 m/s over a run — the marathon world
    -- record is under 6 — and below 0.5 m/s it is not a run. This is not
    -- anti-cheat, it is a bin for nonsense. The board is unverifiable in
    -- principle, the same as the game's leaderboard, and a figure nobody
    -- believes is worth nothing anyway.
    CONSTRAINT logged_runs_speed_sane
        CHECK (distance_m::numeric / duration_s BETWEEN 0.5 AND 8.0)
);

CREATE INDEX idx_logged_runs_runner ON logged_runs (runner_id, started_at DESC);

ALTER TABLE runners            ENABLE ROW LEVEL SECURITY;
ALTER TABLE runners            FORCE  ROW LEVEL SECURITY;
ALTER TABLE runner_credentials ENABLE ROW LEVEL SECURITY;
ALTER TABLE runner_credentials FORCE  ROW LEVEL SECURITY;
ALTER TABLE runner_sessions    ENABLE ROW LEVEL SECURITY;
ALTER TABLE runner_sessions    FORCE  ROW LEVEL SECURITY;
ALTER TABLE logged_runs        ENABLE ROW LEVEL SECURITY;
ALTER TABLE logged_runs        FORCE  ROW LEVEL SECURITY;

-- A username is a byline on a public board, so anyone may read it. The
-- CHECK above is what bounds the open INSERT: a signup can only create
-- a name of the right shape, and the UNIQUE stops it being one already
-- taken.
CREATE POLICY runners_public_select ON runners FOR SELECT USING (true);
CREATE POLICY runners_public_insert ON runners FOR INSERT WITH CHECK (true);

-- Never publicly readable. Login reads this under an admin transaction,
-- the same way admin login reads `admins` — one statement, by username,
-- and nothing else in this codebase touches the table.
CREATE POLICY runner_credentials_admin_select ON runner_credentials FOR SELECT
    USING (current_setting('app.is_admin', true) = 'true');
CREATE POLICY runner_credentials_insert ON runner_credentials FOR INSERT
    WITH CHECK (true);

-- USING(true) for the same reason user_sessions has it: the middleware
-- must resolve a token to an identity before any context exists. The
-- boundary is the application — only src/http/middleware.rs reads this,
-- and always by token_hash.
CREATE POLICY runner_sessions_select ON runner_sessions FOR SELECT USING (true);
CREATE POLICY runner_sessions_insert ON runner_sessions FOR INSERT WITH CHECK (true);
CREATE POLICY runner_sessions_own_delete ON runner_sessions FOR DELETE
    USING (runner_id = NULLIF(current_setting('app.runner_id', true), '')::uuid);

-- The board is public — that is the whole point of it — so runs are
-- readable by anyone. What a run contains is a distance, a duration and
-- a date; there is no route in it to expose.
CREATE POLICY logged_runs_public_select ON logged_runs FOR SELECT USING (true);
CREATE POLICY logged_runs_own_insert ON logged_runs FOR INSERT
    WITH CHECK (runner_id = NULLIF(current_setting('app.runner_id', true), '')::uuid);
CREATE POLICY logged_runs_own_delete ON logged_runs FOR DELETE
    USING (runner_id = NULLIF(current_setting('app.runner_id', true), '')::uuid);

GRANT SELECT, INSERT         ON runners            TO bf_app;
GRANT SELECT, INSERT         ON runner_credentials TO bf_app;
GRANT SELECT, INSERT, DELETE ON runner_sessions    TO bf_app;
GRANT SELECT, INSERT, DELETE ON logged_runs        TO bf_app;

INSERT INTO schema_migrations (version) VALUES ('0039_runners')
ON CONFLICT (version) DO NOTHING;
