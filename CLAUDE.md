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
  `domain::pairings`, `domain::submissions`, `domain::lines`,
  `domain::members`.
  The `/admin/...` route prefixes inside those modules are
  admin-authed but still business logic of that domain.
- **One endpoint accepts unauthenticated writes:**
  `POST /v1/submissions`, where columns and photographs are sent in
  for the magazine. Everything about the `submissions` policies exists
  to bound it — read migration 0018 before changing anything in
  `domain::submissions`. The short version: INSERT is open but only as
  `status = 'pending'`, and there is no non-admin SELECT policy at
  all, so nothing public can read the table in any state.

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
`submissions`. Nothing public may read that table, so
`repository::insert_submission` generates the UUID in Rust and inserts
it explicitly instead of reading one back. If you add another
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

## Audit

`audit::write` always takes the pool, never a transaction. This is
deliberate: when a request rolls back, the audit row stays. The
`audit_events` table is append-only at the DB level — no UPDATE/DELETE
grant for `bf_app`, plus a trigger that raises on either op.

Whenever you add a new mutating endpoint, add a matching `audit::write`
call on the success path. Use the actor_type / action conventions
that already exist (`user.register`, `challenge.issued`, `user.verify`,
`event.create`, `admin.login`, `maintenance.prune`,
`pairing.created`, `pairing.decided`, `submission.received`,
`line.published`).

`actor_type` is `'user' | 'admin' | 'system' | 'public'`. `'public'`
is what `submission.received` is written as: no user, no admin, no
server. Use it for anything else genuinely anonymous rather than
mislabelling it `'system'`.

Note `submission.rejected` is the audit trail for a *deletion*: the
row, the writing and any photograph are gone, and this entry is the
only remaining record that the submission ever existed.

## Two tables, opposite directions

`submissions` and `taste_lines` look similar and are mirror images.
Getting them the wrong way round would either publish somebody's
private correspondence or let anyone write the magazine:

| | write | read |
|---|---|---|
| `submissions` (0018, 0020, 0023) | **members only, as their own `pending`** | own rows always; `accepted` public |
| `taste_lines` (0019) | **admin only** | public, only where `published` |

Both are moderated publication, and nobody may publish themselves.
0023 closed the write side of submissions: the INSERT policy checks
`user_id` against `app.user_id`, so a transaction without a member's
context cannot write at all. That is the enforcement — not the
handler, which merely fails earlier and more politely. 0018 originally made submissions private correspondence;
0020 corrected that — they are posts, and the magazine is a selection
from what is already public.

`submissions.submitter_email` is the exception and stays editor-only.
RLS is row-level, so the accepted-row policy does expose that column
to any query the app makes — the boundary is the repository, which
names its columns. Never `SELECT *` on a public submissions path.

## A member is a number

Nothing public about a member is chosen by them. `users.member_no` is
sequential in the order people were verified, and it is the byline on
every post. There is no public name.

`display_name` still exists and is used by exactly one thing:
pairings, where two people who already met and both said yes can put
a name to each other. That is a private channel after consent, not a
public identity.

Two paths mint a number, and both must: an admin signing somebody up
at a run (`domain::members`) and the iOS device proving itself at an
event (`domain::onboarding`). The `users_member_no_matches_status`
CHECK from 0024 makes forgetting one impossible — it is what caught
the second path when only the first had been written.

## A membership is kept, not got

Turning up opens the account; turning up every month keeps it able to
post. 0025 puts that in the INSERT policy through
`bf_member_is_current` — a SECURITY DEFINER function, because
`attendances` is under RLS and a posting member has no context to
read it.

A lapse takes nothing away. The account, the number and everything
they wrote stay exactly where they are; only posting stops, and it
resumes the moment an admin marks them present again.

Attendance is recorded by an admin typing a member number while
looking at the person. There is deliberately no code to scan: a code
on a screen can be photographed and sent to somebody at home.

`bf_app` has no UPDATE on `attendances`. When somebody was somewhere
is a fact, not a field — record it or delete it.

## The channel is the one thing the server cannot read

Messages arrive as AES-GCM ciphertext and are stored as ciphertext.
The key is derived in the two browsers from a static ECDH exchange
and never reaches this process — no column holds plaintext, and there
is a test that fails if one appears.

What is visible is metadata: that two members exchanged something,
how much, and when. That is unavoidable. The words are not.

`bf_app` has SELECT and INSERT on `messages` and nothing else. A
message cannot be edited or unsaid, by anybody, including an admin.
There is deliberately no audit entry either — `audit_events` is
append-only, so a record of who wrote to whom could never come out.

Reading requires the pairing to still be open, via
`bf_pairing_is_open_for`. Blocking therefore ends the conversation
for both people rather than leaving one side with a transcript.

Static ECDH means no forward secrecy: a stolen private key reads
everything that member received. Ratcheting would fix it and is a
much larger machine.

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
