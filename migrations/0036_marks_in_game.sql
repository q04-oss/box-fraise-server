-- One purchase, two placements.
--
-- A business already uploads artwork once and it becomes a mark the
-- scanner recognises on a poster. This flag puts the same image on a
-- billboard in Whyte, the game on /whyte and /os, so buying an
-- advertisement means a thing in the street somebody can scan and a
-- thing in a game somebody runs past — without a second product, a
-- second upload or a second panel.
--
-- Off by default. The strawberry and the run club's own poster are
-- marks too, and neither belongs on a billboard unless somebody says
-- so.
ALTER TABLE marks ADD COLUMN in_game BOOLEAN NOT NULL DEFAULT false;

CREATE INDEX idx_marks_in_game ON marks (sort_order, created_at)
    WHERE published AND in_game;

INSERT INTO schema_migrations (version) VALUES ('0036_marks_in_game')
ON CONFLICT (version) DO NOTHING;
