-- What an advertisement costs, and a way for a business to send one in.
--
-- Two things here.
--
-- 1. `adverts.price_cents` — what the advertiser pays per open, next to
--    `pays_cents`, which is what the person who opens it receives. Both
--    are recorded because both are real: the difference is what the
--    platform keeps, and a system that only stores the payout cannot
--    tell you what it earned. The standing price is $3 an open with $1
--    of it going to the reader; nothing enforces that ratio here, so it
--    can be changed for one advertiser without a migration.
--
-- 2. `advert_requests` — a business outlines an advertisement they have.
--    This is an open write path, the second one on the platform, and it
--    is bounded the same way the first one was: INSERT only, only as
--    `pending`, and **no public SELECT policy at all**, so nothing
--    unauthenticated can read the table back in any state. A request is
--    not an advert and never becomes one by itself — an admin turns it
--    into one, which is the moment money has changed hands.
--
-- Contact details are collected here and nowhere else on the running
-- layer. A runner gives no email and is never asked for one; a business
-- has to be reachable, because somebody has to send them an invoice and
-- tell them when it went up.

ALTER TABLE adverts
    ADD COLUMN price_cents INTEGER NOT NULL DEFAULT 300;

ALTER TABLE adverts
    ADD CONSTRAINT adverts_price_sane
    CHECK (price_cents > 0 AND price_cents <= 100000 AND price_cents >= pays_cents);

CREATE TABLE advert_requests (
    id           UUID PRIMARY KEY,
    advertiser   TEXT NOT NULL,
    -- How to reach them. An email or a phone number; not validated,
    -- because a business that mistypes it is a business you telephone.
    contact      TEXT NOT NULL,
    teaser       TEXT NOT NULL,
    body         TEXT NOT NULL,
    link         TEXT,
    -- How many opens they want to buy. What they will be invoiced is
    -- this times the price.
    opens_wanted INTEGER NOT NULL,
    status       TEXT NOT NULL DEFAULT 'pending',
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT advert_requests_status_known CHECK (status IN ('pending', 'accepted')),
    CONSTRAINT advert_requests_opens_sane
        CHECK (opens_wanted > 0 AND opens_wanted <= 100000),
    CONSTRAINT advert_requests_text_sane CHECK (
        length(advertiser) BETWEEN 1 AND 80 AND
        length(contact)    BETWEEN 3 AND 200 AND
        length(teaser)     BETWEEN 1 AND 140 AND
        length(body)       BETWEEN 1 AND 4000
    )
);

CREATE INDEX idx_advert_requests_pending ON advert_requests (created_at)
    WHERE status = 'pending';

ALTER TABLE advert_requests ENABLE ROW LEVEL SECURITY;
ALTER TABLE advert_requests FORCE  ROW LEVEL SECURITY;

-- Anybody may send one, and only as pending. There is deliberately no
-- non-admin SELECT policy, so nothing public can read this table in any
-- state — not their own row, not anybody else's.
--
-- Because the writer cannot read back what it just wrote, `INSERT ...
-- RETURNING` would fail with 42501 here. The id is generated in Rust and
-- inserted explicitly. Do not "fix" that by widening the SELECT policy.
CREATE POLICY advert_requests_public_insert ON advert_requests FOR INSERT
    WITH CHECK (status = 'pending');
CREATE POLICY advert_requests_admin_select ON advert_requests FOR SELECT
    USING (current_setting('app.is_admin', true) = 'true');
CREATE POLICY advert_requests_admin_update ON advert_requests FOR UPDATE
    USING (current_setting('app.is_admin', true) = 'true');
CREATE POLICY advert_requests_admin_delete ON advert_requests FOR DELETE
    USING (current_setting('app.is_admin', true) = 'true');

GRANT SELECT, INSERT, UPDATE, DELETE ON advert_requests TO bf_app;

INSERT INTO schema_migrations (version) VALUES ('0041_advert_requests')
ON CONFLICT (version) DO NOTHING;
