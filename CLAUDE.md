# Box Fraise MVP — operating manual for future contributors

This document encodes the discipline that makes this codebase safe.
Read it before changing routes, transactions, RLS policies, or anything
in `src/db.rs`. Drift from these conventions is exactly how the bugs
this rewrite was designed to avoid show up.

## Surface

- All API routes are versioned under `/v1`. The only exception is
  `GET /admin`, which serves the static admin tool — not an API.
- Route-owning modules: `domain::admin` (login), `domain::onboarding`
  (register, challenge, verify, me), `domain::events` (public + admin
  events), `domain::businesses` (directory), `domain::consultations`,
  `domain::stickers` (sticker map), `domain::sites` (strawberry
  camera spots).
  The `/admin/...` route prefixes inside those modules are
  admin-authed but still business logic of that domain.
- **One endpoint accepts unauthenticated writes:**
  `POST /v1/stickers/{slug}/photos`. Everything about the
  `sticker_photos` policies exists to bound it — see "The public write
  boundary" below before changing anything in `domain::stickers`.

## Architecture in three layers

```
routes/    ← thin HTTP edges; no DB, no policy decisions
service/   ← business logic; OPENS and OWNS transactions; calls audit
repository/← SQL only; takes &mut PgConnection (or RlsTransaction.conn())
```

A handler is allowed to deserialize input, call one service, and
serialize output. It does not touch the pool, talk to the DB, or
inspect headers beyond what an extractor pre-resolved.

A service decides the transaction kind (Rls / AdminRls / plain),
does the work, commits, and emits audit. **Audit writes happen
outside the transaction**, on the bare pool, so the trail survives a
rollback.

A repository function takes `&mut PgConnection` (or
`RlsTransaction.conn()`), runs one SQL statement, returns the row.
It does not own a transaction, does not make policy decisions, does
not log.

## Two-role Postgres model

- `postgres` (the docker compose superuser) owns every object. The
  migration runs as this role. The app never connects as it. Owners
  bypass RLS, so anything that talks to the DB as the owner is
  outside the safety model.
- `bf_app` (created in `docker/init/01-roles.sql`) is the runtime
  identity. No BYPASSRLS. Sees only the rows policies permit and can
  only execute the verbs explicitly granted at the bottom of the
  migration.
- Every table has `FORCE ROW LEVEL SECURITY` so RLS applies even if a
  connection coincides with the owner role. Belt and suspenders.

If you ever need to grant the app a new verb, update the GRANT block
at the bottom of `migrations/0001_init.sql`. Do not work around RLS
by switching the runtime to a more privileged role.

## The RLS / GUC contract

Two transaction-local GUCs drive every policy:

| GUC             | Set by                       | Used in policies that say                                       |
|-----------------|------------------------------|------------------------------------------------------------------|
| `app.user_id`   | `RlsTransaction::begin`      | `id = NULLIF(current_setting('app.user_id', true), '')::uuid`    |
| `app.is_admin`  | `AdminRlsTransaction::begin` | `current_setting('app.is_admin', true) = 'true'`                 |

**Always `is_local = true`.** Use `set_config('app.user_id', $1, true)`
or the wrapper. A non-local SET outlives the request and leaks across
pool connections — that's the historic source of `/me returns empty`
and cross-user data exposure.

**NULLIF guard.** Once `set_config` has touched a GUC on a
connection, the slot is allocated. After commit the slot reverts to
empty string, NOT NULL. Casting `''::uuid` raises 22P02 and breaks
every query in the request. Every policy that compares a UUID wraps
the read in `NULLIF(current_setting(...), '')::uuid`. New policies
must do the same.

**Never set `app.user_id` to an empty string from Rust.** Always pass
a real UUID or skip the call.

**`INSERT ... RETURNING` is subject to the SELECT policy.** Postgres
applies the table's SELECT policies to the row an INSERT returns, so
on a table where the writer cannot read back what it just wrote, a
`RETURNING` clause fails with 42501 — *"new row violates row-level
security policy"* — even though the insert itself is permitted. The
message points at WITH CHECK and sends you looking in the wrong
place.

This bites exactly where a write path is deliberately blind:
`sticker_photos`. The public submit inserts a `pending` row that
`sticker_photos_public_select` hides, so
`repository::insert_photo` generates the UUID in Rust and inserts it
explicitly instead of reading one back. If you add another
write-without-read path, do the same — do not "fix" it by widening
the SELECT policy.

**Sessions-table SELECT is intentionally wide.** The auth middleware
has to resolve `Bearer <token>` → identity before any user context
exists. `user_sessions` and `admin_sessions` therefore have a
`USING(true)` SELECT policy. The audit boundary is the application:
only `src/http/middleware.rs` reads these tables, and it always
filters by `token_hash`. Do not add other read paths there.

## Verify flow (the climax of onboarding)

1. iOS app generates a P-256 keypair in the Secure Enclave.
2. `POST /v1/onboard/register` → service creates a pending user,
   stores the public key (SEC1 uncompressed, 65 bytes), returns a
   session token.
3. iOS app calls `POST /v1/onboard/challenge` → server issues a
   short-lived nonce bound to that user.
4. iOS app signs the nonce with the Secure Enclave key and shows the
   QR `{nonce, signature_b64}`.
5. Admin scans at the event: `POST /v1/admin/verify` with
   `{nonce, signature_b64, event_id}`.
6. Server looks up the challenge, fetches the device's public key,
   verifies the signature (P-256 / DER / SHA-256 prehash / low-S
   normalised — see crypto.rs), and runs the atomic flip:

   ```sql
   UPDATE users
      SET status='verified', verified_at=now(),
          verified_at_event_id=$1, verified_by_admin_id=$2
    WHERE id=$3 AND status='pending'
    RETURNING verified_at;
   ```

   That `WHERE id=$3 AND status='pending'` is the race-close. Two
   admins scanning simultaneously: exactly one UPDATE returns a row,
   the other returns zero → 409 Conflict.

## P-256 + iOS specifics

- Public keys are SEC1 uncompressed: `0x04 || X(32) || Y(32)` = 65
  bytes. Register validates length AND that the bytes parse as a real
  P-256 point.
- Signatures from iOS `SecKeyCreateSignature(...,
  .ecdsaSignatureMessageX962SHA256, ...)` are **DER-encoded** and
  **may be high-S**. Our verifier (`crypto::verify_p256_signature`)
  normalises S before checking. Do not switch to a "strict low-S
  only" verify or Apple signatures will fail.
- The crypto crate `p256 = "0.13"` is the source of truth. If you
  upgrade, re-check that `Signature::from_der`, `normalize_s`, and
  `VerifyingKey::verify` still hash the message with SHA-256 by
  default (they do as of 0.13).

## The public write boundary (sticker photos)

`POST /v1/stickers/{slug}/photos` takes a photo from anyone, with no
token. The containment is four independent layers, and the feature is
only safe while all four hold:

1. **RLS pins the status.** `sticker_photos_public_insert` has
   `WITH CHECK (status = 'pending' AND reviewed_at IS NULL AND
   reviewed_by_admin_id IS NULL AND EXISTS (published parent))`. A
   submitter cannot self-publish or forge a review trail. The service
   also hardcodes `'pending'` rather than reading it from the request,
   so there are two independent barriers, not one.
2. **RLS hides the row.** `sticker_photos_public_select` admits
   `status = 'approved'` only. A pending photo is invisible to every
   public read path including the byte-serving endpoint, so guessing a
   UUID gains nothing.
3. **The content type comes from the bytes.** `sniff_content_type`
   reads magic bytes and ignores what the client declared. The
   allowlist is JPEG/PNG/WebP. **SVG is excluded on purpose** — it is
   a script container, and serving one from our origin would be stored
   XSS. Do not add it.
4. **Size is capped three times** — `DefaultBodyLimit` on the route,
   `MAX_IMAGE_BYTES` in the service, and the `sticker_photos_size_cap`
   CHECK. Change one, change all three.

Two things this does *not* do, deliberately: it never decodes the
image (magic bytes + length only, so a well-headed garbage file is
storable and the moderation queue is what catches it), and it does not
strip EXIF server-side. The map page re-encodes through a canvas
before upload, which drops GPS tags for every normal browser
submission, but a crafted client can still post EXIF-bearing JPEG.
If that matters, re-encode server-side with the `image` crate.

The only rate limiting is `MAX_PENDING_PER_STICKER`, enforced through
the `bf_sticker_pending_photo_count` SECURITY DEFINER function —
needed because a public transaction cannot see pending rows to count
them. Do not replace it by opening an `AdminRlsTransaction` on a
public request.

## Sites, and the location that is never collected

`sites` are places a visitor can stand and see a strawberry through
their phone camera. The table holds only where the sites are.

**There is deliberately no check-in table, and no column anywhere
recording who went where.** Whether a visitor is close enough is
decided in their browser: the page reads a position from the
Geolocation API, compares it to the site's published coordinates, and
discards it. Nothing is transmitted, so nothing can leak, be
subpoenaed, or need a retention policy. If a future feature seems to
need "who visited", that is a design change with a privacy cost, not a
missing column — treat it as such.

The corollary: the proximity check is **not a security boundary**.
Anyone with devtools can fake a position. That is acceptable because
nothing is at stake — there is no reward, no account, and no record.
Do not build anything on top of it that assumes the visitor really was
there.

Sites are stored separately from stickers rather than as a `kind`
column, because `sticker_photos` has a foreign key to `stickers` and a
site can never have photos. One table would have meant rows that are
structurally allowed to have children they can never legitimately
have.

## Audit

`audit::write` always takes the pool, never a transaction. This is
deliberate: when a request rolls back, the audit row stays. The
`audit_events` table is append-only at the DB level — no UPDATE/DELETE
grant for `bf_app`, plus a trigger that raises on either op.

Whenever you add a new mutating endpoint, add a matching `audit::write`
call on the success path. Use the actor_type / action conventions
that already exist (`user.register`, `challenge.issued`, `user.verify`,
`event.create`, `admin.login`, `maintenance.prune`, `sticker.create`,
`sticker_photo.submit`, `sticker_photo.approve`,
`sticker_photo.reject`, `site.create`).

`actor_type` is `'user' | 'admin' | 'system' | 'public'`. `'public'`
was added in migration 0012 for anonymous photo submissions — an
actor that is none of the other three. Use it rather than mislabelling
an anonymous action as `'system'`.

Note `sticker_photo.reject` is the audit trail for a *deletion*: the
row and its bytes are gone, and this entry is the only remaining
record that the submission ever existed.

## When you change a table

1. Add the table to `migrations/0001_init.sql` (or a new
   `0002_*.sql`).
2. Add `ENABLE ROW LEVEL SECURITY` + `FORCE ROW LEVEL SECURITY`.
3. Write policies for the verbs the app needs. If a UUID comparison
   is involved, wrap the GUC read in `NULLIF(..., '')::uuid`.
4. Add the GRANT to `bf_app` in the grants block.
5. If the table holds something audit-worthy, write the audit on
   the success path of the mutating service.
6. Add an integration test that asserts the isolation property of
   the new policy.
