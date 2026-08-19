-- Three prompts, and the end of taste_lines.
--
-- Until now every post answered one question, and the question lived
-- nowhere but an <h1> on /gurgle. Nothing in the database knew what a
-- post was an answer to. That was fine with one prompt and is wrong
-- with three.
--
-- The three:
--
--   run_country   Do I Have What it Takes To Run This Country?
--   run_away      Why Do I Want To Run Away?
--   better_taste  for better taste…
--
-- All three are about running, which is the joke and the reason they
-- belong to the same platform. The site does not explain the third one
-- and should not start: a gloss would kill it.
--
-- Existing rows are backfilled to run_country because that is what they
-- answered — it was the only question there was.
ALTER TABLE submissions ADD COLUMN prompt TEXT;
UPDATE submissions SET prompt = 'run_country' WHERE prompt IS NULL;
ALTER TABLE submissions ALTER COLUMN prompt SET NOT NULL;

-- A CHECK rather than a lookup table. These are editorial copy, not
-- data: they change when the magazine changes its mind, which is a
-- migration either way, and a three-row table with a foreign key would
-- add a join to every read of the feed for nothing.
ALTER TABLE submissions ADD CONSTRAINT submissions_prompt_known
    CHECK (prompt IN ('run_country', 'run_away', 'better_taste'));

CREATE INDEX idx_submissions_prompt_accepted
    ON submissions (prompt, submitted_at DESC)
    WHERE status = 'accepted';

-- taste_lines goes.
--
-- It was the pool the strawberry sticker drew from: lines written by an
-- editor, completing "for better taste:". Now that members answer that
-- prompt themselves, keeping both would mean two different things with
-- the same name — an admin pool behind the sticker and a member prompt
-- on gurgle — permanently confused for each other.
--
-- So the sticker draws from what members wrote, once an editor has
-- accepted it. That is also the scannable-advertisement thesis doing
-- what it says: you point a camera at something in the street, and what
-- comes back was made by somebody who turned up to a run.
--
-- 0019 created it, 0019's grants and policies go with it.
DROP TABLE IF EXISTS taste_lines;
