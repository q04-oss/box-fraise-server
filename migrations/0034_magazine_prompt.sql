-- Anonymous pieces answer one of the three, like everything else.
--
-- 0033 took open-ended writing, which made the first edition a
-- different shape from every edition after it: members answer one of
-- three questions, and /magazine says so two paragraphs above the box.
-- A first edition assembled from unprompted writing could not sit
-- beside member answers without reading as a different magazine.
--
-- Same three as `submissions.prompt` in 0028, and the same CHECK. The
-- canonical list lives in Prompt::ALL in Rust and web/js/prompts.js in
-- the browser; this is the third place and the one that has the final
-- say.
ALTER TABLE magazine_submissions ADD COLUMN prompt TEXT;
UPDATE magazine_submissions SET prompt = 'run_country' WHERE prompt IS NULL;
ALTER TABLE magazine_submissions ALTER COLUMN prompt SET NOT NULL;

ALTER TABLE magazine_submissions ADD CONSTRAINT magazine_submissions_prompt_known
    CHECK (prompt IN ('run_country', 'run_away', 'better_taste'));

INSERT INTO schema_migrations (version) VALUES ('0034_magazine_prompt')
ON CONFLICT (version) DO NOTHING;
