-- The leaderboard for Whyte, the game on /os.
--
-- Deliberately outside the platform's identity system. Everything else
-- here is earned by turning up: you post under a member number, and a
-- number means somebody made you an account while looking at you. This
-- is a game on a mock desktop and applying that rule to it would be
-- taking a toy seriously.
--
-- So: three letters, the way an arcade cabinet did it. No account, no
-- session, no cookie. Anybody who plays can be on it.
--
-- Both obvious abuses are accepted rather than engineered against. The
-- score is client-reported and cannot be verified without simulating
-- the run server-side, which is not worth it for a game about jumping
-- over cars. And three letters can spell something unpleasant — the
-- answer is that an admin deletes the row, not that the table gets
-- clever. The CHECK below is for garbage, not for people.

CREATE TABLE whyte_scores (
    id          UUID PRIMARY KEY,
    -- Exactly three letters, uppercase, nothing else. Narrow on
    -- purpose: it is the whole of what a stranger can write here.
    initials    TEXT NOT NULL,
    metres      INTEGER NOT NULL,
    achieved_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT whyte_scores_initials_shape CHECK (initials ~ '^[A-Z]{3}$'),
    CONSTRAINT whyte_scores_sane CHECK (metres > 0 AND metres <= 100000)
);

CREATE INDEX idx_whyte_scores_board ON whyte_scores (metres DESC, achieved_at ASC);

ALTER TABLE whyte_scores ENABLE ROW LEVEL SECURITY;
ALTER TABLE whyte_scores FORCE ROW LEVEL SECURITY;

-- Anyone may read it and anyone may add to it. This is the second
-- unauthenticated write on the platform and by far the least
-- consequential: three letters and an integer.
CREATE POLICY whyte_scores_public_select ON whyte_scores FOR SELECT USING (true);
CREATE POLICY whyte_scores_public_insert ON whyte_scores FOR INSERT WITH CHECK (true);
CREATE POLICY whyte_scores_admin_delete ON whyte_scores FOR DELETE
    USING (current_setting('app.is_admin', true) = 'true');

GRANT SELECT, INSERT, DELETE ON whyte_scores TO bf_app;

INSERT INTO schema_migrations (version) VALUES ('0035_whyte_scores')
ON CONFLICT (version) DO NOTHING;
