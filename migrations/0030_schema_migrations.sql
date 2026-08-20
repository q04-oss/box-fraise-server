-- What the database has actually had done to it.
--
-- Migrations here are applied by hand, with psql, against whichever
-- database somebody happened to be pointed at. Nothing recorded which
-- ones had run, and nothing checked. The failure that produces is
-- silent and total: 0028 added a `prompt` column and the code that
-- selects it shipped to production before the migration did, so the
-- gurgle feed answered `{"error":"internal"}` to every reader until
-- somebody happened to look. The database was fine. The code was fine.
-- They were simply not the same age.
--
-- So: a table that says what has run, and a check at boot that refuses
-- to start the server when the code is ahead of the schema. A deploy
-- that would have served 500s now fails loudly at startup instead,
-- which is the difference between an outage and an alert.
--
-- The app cannot apply migrations itself and should not be able to.
-- bf_app has no CREATE privilege, and giving it owner credentials to
-- fix a deployment problem would hand every runtime bug the ability to
-- rewrite the schema. The two-role model is worth more than the
-- convenience. So this is a check, not a runner: the human still runs
-- the SQL, and the server declines to lie about it afterwards.

CREATE TABLE schema_migrations (
    version    TEXT PRIMARY KEY,
    applied_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Every migration from here on ends by recording itself, which is why
-- this file's own row is at the bottom. The convention is in CLAUDE.md;
-- forgetting it makes the boot check fail, which is the intended
-- direction to fail in.
INSERT INTO schema_migrations (version) VALUES
    ('0001_init'),
    ('0002_multi_key_and_staff'),
    ('0003_schedule'),
    ('0004_celestial'),
    ('0005_consultations_and_cards'),
    ('0006_hair_profiles_and_model_requests'),
    ('0007_event_questions'),
    ('0008_prune_non_mvp'),
    ('0009_businesses'),
    ('0010_business_fields'),
    ('0011_event_poster'),
    ('0012_stickers'),
    ('0013_hosts_and_sites'),
    ('0014_sightings'),
    ('0015_pairings'),
    ('0016_drop_sites'),
    ('0017_drop_sightings'),
    ('0018_submissions'),
    ('0019_taste_lines'),
    ('0020_publish_submissions'),
    ('0021_submissions_require_email'),
    ('0022_drop_submitter_email'),
    ('0023_submissions_belong_to_members'),
    ('0024_member_numbers'),
    ('0025_attendance_keeps_membership'),
    ('0026_messages'),
    ('0027_sessions_and_sign_out'),
    ('0028_prompts'),
    ('0029_calendar')
ON CONFLICT (version) DO NOTHING;

-- Read-only to the app, and readable before any context exists — the
-- check runs at boot, before there is a request, let alone a user. It
-- holds no personal data of any kind: filenames and timestamps.
ALTER TABLE schema_migrations ENABLE ROW LEVEL SECURITY;
ALTER TABLE schema_migrations FORCE ROW LEVEL SECURITY;
CREATE POLICY schema_migrations_read ON schema_migrations FOR SELECT USING (true);

GRANT SELECT ON schema_migrations TO bf_app;

INSERT INTO schema_migrations (version) VALUES ('0030_schema_migrations')
ON CONFLICT (version) DO NOTHING;
