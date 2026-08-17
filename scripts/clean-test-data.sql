-- Remove everything the integration suite leaves behind.
--
-- Run against a DEV database when the test rows have piled up:
--
--   docker exec -i box-fraise-postgres-1 \
--     psql -U postgres -d box_fraise -v ON_ERROR_STOP=1 \
--     < scripts/clean-test-data.sql
--
-- Safe by construction rather than by care: every row it touches is
-- reachable only from an admin whose email ends '@test.local' or a
-- device key whose key_id is 'test-device'. Neither exists outside a
-- test run, so pointing this at production deletes nothing. It is
-- still not something to point at production.
--
-- The order below is the foreign-key graph, children first. Several
-- edges are ON DELETE NO ACTION — events.created_by_admin_id,
-- users.verified_by_admin_id, users.verified_at_event_id, and most of
-- the staff/salon/consultation columns — so the parents cannot go
-- until the rows naming them are gone. If a new table gains a NO
-- ACTION reference to users, events or admins, it needs a line here or
-- this script starts failing.
--
-- This is deliberately NOT wired into the test suite. Cleanup that
-- runs inside a parallel suite races with tests still using the rows,
-- and cleanup that knows the whole FK graph is one more thing to keep
-- in step with the schema. Running it by hand, occasionally, is the
-- cheaper trade.

BEGIN;

CREATE TEMP TABLE t_admins ON COMMIT DROP AS
    SELECT id FROM admins WHERE email LIKE '%@test.local';

CREATE TEMP TABLE t_users ON COMMIT DROP AS
    SELECT DISTINCT u.id
      FROM users u
      JOIN device_keys dk ON dk.user_id = u.id
     WHERE dk.key_id = 'test-device'
    UNION
    SELECT u.id FROM users u WHERE u.verified_by_admin_id IN (SELECT id FROM t_admins);

CREATE TEMP TABLE t_events ON COMMIT DROP AS
    SELECT id FROM events WHERE created_by_admin_id IN (SELECT id FROM t_admins);

-- ── Rows naming a test user ─────────────────────────────────────────
DELETE FROM staff
 WHERE user_id                      IN (SELECT id FROM t_users)
    OR promoted_by_user_id          IN (SELECT id FROM t_users)
    OR consultation_trainer_user_id IN (SELECT id FROM t_users);

UPDATE identity_cards SET replaced_by_card_id = NULL
 WHERE user_id           IN (SELECT id FROM t_users)
    OR issued_by_user_id IN (SELECT id FROM t_users);

DELETE FROM identity_cards
 WHERE user_id             IN (SELECT id FROM t_users)
    OR issued_by_user_id   IN (SELECT id FROM t_users);

-- After identity_cards, which references it.
DELETE FROM social_verifications
 WHERE user_id               IN (SELECT id FROM t_users)
    OR consulted_by_user_id  IN (SELECT id FROM t_users);

-- Before salon_appointments, which it references.
DELETE FROM consultation_requests
 WHERE user_id                IN (SELECT id FROM t_users)
    OR responded_by_user_id   IN (SELECT id FROM t_users);

DELETE FROM salon_appointments
 WHERE user_id              IN (SELECT id FROM t_users)
    OR created_by_user_id   IN (SELECT id FROM t_users)
    OR cancelled_by_user_id IN (SELECT id FROM t_users)
    OR stylist_user_id      IN (SELECT id FROM t_users);

DELETE FROM professional_licenses WHERE user_id IN (SELECT id FROM t_users);
DELETE FROM pairings
 WHERE lower_user_id IN (SELECT id FROM t_users)
    OR upper_user_id IN (SELECT id FROM t_users)
    OR closed_by     IN (SELECT id FROM t_users);
DELETE FROM pairing_nonces WHERE initiator_id IN (SELECT id FROM t_users);
DELETE FROM user_sessions  WHERE user_id      IN (SELECT id FROM t_users);
DELETE FROM challenges     WHERE user_id      IN (SELECT id FROM t_users);
DELETE FROM device_keys    WHERE user_id      IN (SELECT id FROM t_users);

-- ── The users themselves, then what they were verified at ──────────
DELETE FROM users WHERE id IN (SELECT id FROM t_users);

-- Any surviving user still pointing at a test event would block it.
UPDATE users
   SET verified_at_event_id = NULL
 WHERE verified_at_event_id IN (SELECT id FROM t_events);

DELETE FROM events          WHERE id       IN (SELECT id FROM t_events);

-- ── The admins ─────────────────────────────────────────────────────
-- Anything they reviewed or wrote is ON DELETE SET NULL, so the work
-- itself survives with its reviewer forgotten. Test runs should not
-- have left any real work behind, but not destroying it is the right
-- default.
DELETE FROM admin_sessions WHERE admin_id IN (SELECT id FROM t_admins);
DELETE FROM admins         WHERE id       IN (SELECT id FROM t_admins);

COMMIT;
