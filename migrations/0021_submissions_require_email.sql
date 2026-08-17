-- =============================================================
-- 0021 — Posting requires an email.
--
-- 0018 collected `submitter_contact` as optional free text, so people
-- could leave an Instagram handle, a phone number, or nothing. That
-- made a post unattachable to anybody.
--
-- An email at the moment of posting is what makes an account possible
-- later: the post is already tied to a handle, so a magic link can
-- claim it rather than asking somebody to sign up for nothing. The
-- email is the identity handle for this project — a device key, when
-- the iOS app exists, is a credential attached to it, not a second
-- identity.
--
-- This costs nothing right now because no submission exists anywhere.
-- Once one does it needs a backfill decision, which is the reason to
-- do it today.
--
-- What it gives up: anonymous posting. Somebody who wants to send a
-- photograph without handing over an address no longer can.
--
-- The CHECK is deliberately loose. It rejects what is obviously not an
-- address — no @, whitespace, nothing either side — and no more than
-- that. Regexes that try to encode RFC 5322 reject real addresses, and
-- the only proof an address works is that a message sent to it arrives.
-- =============================================================

BEGIN;

ALTER TABLE submissions RENAME COLUMN submitter_contact TO submitter_email;

ALTER TABLE submissions RENAME CONSTRAINT submissions_contact_len
                                       TO submissions_email_len;

-- No rows exist yet, so this needs no backfill. If that ever stops
-- being true, this statement is where it breaks — deliberately, rather
-- than quietly inventing an address for somebody.
ALTER TABLE submissions ALTER COLUMN submitter_email SET NOT NULL;

ALTER TABLE submissions ADD CONSTRAINT submissions_email_shape
    CHECK (submitter_email ~ '^[^@[:space:]]+@[^@[:space:]]+\.[^@[:space:]]+$');

COMMENT ON COLUMN submissions.submitter_email IS
    'Editor only, and the handle a magic link would claim this post with. '
    'Never returned on a public read path — see 0020.';

COMMIT;
