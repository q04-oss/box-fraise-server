-- Advertisements the scanner knows about.
--
-- Until now the list lived in a JavaScript array inside
-- web/scan/index.html. Selling a business a scannable poster meant
-- editing that file and redeploying, which is fine for the first one
-- and not a product.
--
-- A mark is a picture and what happens when somebody points a camera at
-- it. The picture is stored here as bytes, the way submissions are,
-- rather than as a path on disk — the runtime has no writable volume and
-- nothing should depend on one.
--
-- The fingerprint is deliberately NOT stored. The scanner computes a
-- dHash of the reference image in the browser, with the same code it
-- uses on the camera frame, and compares the two. Precomputing it here
-- would mean two implementations that have to agree forever, and the
-- day they drift every advertisement in the city stops scanning. The
-- images are a few kilobytes; loading them is cheaper than that risk.
--
-- `act` is what a match does:
--   'go'   navigate to `target`
--   'line' draw an accepted "for better taste…" answer
-- Anything else is refused by the CHECK rather than silently ignored.

CREATE TABLE marks (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    label          TEXT NOT NULL,
    image_bytes    BYTEA NOT NULL,
    content_type   TEXT NOT NULL,
    act            TEXT NOT NULL,
    target         TEXT,
    -- Which business bought it, when one did. Null for the platform's
    -- own marks — the strawberry belongs to nobody.
    business_id    UUID REFERENCES businesses(id) ON DELETE SET NULL,
    published      BOOLEAN NOT NULL DEFAULT true,
    sort_order     INTEGER NOT NULL DEFAULT 0,
    created_by_admin_id UUID REFERENCES admins(id),
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT marks_act_known CHECK (act IN ('go', 'line')),
    -- 'go' without somewhere to go is a mark that does nothing.
    CONSTRAINT marks_go_has_target
        CHECK (act <> 'go' OR (target IS NOT NULL AND target <> '')),
    CONSTRAINT marks_size_cap CHECK (octet_length(image_bytes) <= 2 * 1024 * 1024),
    CONSTRAINT marks_type_known CHECK (content_type IN ('image/jpeg', 'image/png', 'image/webp'))
);

CREATE INDEX idx_marks_published ON marks (sort_order, created_at) WHERE published;

ALTER TABLE marks ENABLE ROW LEVEL SECURITY;
ALTER TABLE marks FORCE ROW LEVEL SECURITY;

-- Anyone may read a published mark: the scanner is a public page and
-- has no session. The bytes are a picture already printed and stuck to
-- a wall in the street, so there is nothing here to protect.
CREATE POLICY marks_public_select ON marks FOR SELECT
    USING (published OR current_setting('app.is_admin', true) = 'true');
CREATE POLICY marks_admin_write ON marks FOR INSERT
    WITH CHECK (current_setting('app.is_admin', true) = 'true');
CREATE POLICY marks_admin_update ON marks FOR UPDATE
    USING (current_setting('app.is_admin', true) = 'true');
CREATE POLICY marks_admin_delete ON marks FOR DELETE
    USING (current_setting('app.is_admin', true) = 'true');

GRANT SELECT, INSERT, UPDATE, DELETE ON marks TO bf_app;

INSERT INTO schema_migrations (version) VALUES ('0031_marks')
ON CONFLICT (version) DO NOTHING;
