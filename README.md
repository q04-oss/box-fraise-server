# box-fraise

Rust/Axum backend for the Box Fraise MVP. One job: onboard a verified,
in-person userbase by having admins scan signed QR codes at hair events.

The architecture, RLS model, and verify flow are documented in
[CLAUDE.md](./CLAUDE.md) — read that before changing anything in
`src/db.rs`, `migrations/0001_init.sql`, or the auth middleware.

## Quick start

```bash
# 1. Start Postgres (creates the `bf_app` runtime role via init script)
docker compose up -d

# 2. Wait for it, then apply migrations.
#    `sqlx-cli` or any psql will do; the migration runs as the owner.
docker exec -i box-fraise-postgres-1 \
  psql -U postgres -d box_fraise < migrations/0001_init.sql

# 3. Set env (see .env.example for the full list)
cp .env.example .env
export $(grep -v '^#' .env | xargs)

# 4. Build and run
cargo run
```

On boot, the server will:

- Connect as `bf_app` (the restricted runtime role).
- **Seed the admin** declared in `SEED_ADMIN_EMAIL` /
  `SEED_ADMIN_PASSWORD` if no admin with that email exists. Without
  this, the admin tool has no one to log in as — there is no admin
  signup endpoint. Rotate the password once a real auth-rotation
  flow lands.
- Start a periodic prune (`maintenance.rs`): expired admin sessions
  + pending users older than 30 days, hourly.

## Surface

| Endpoint                                     | Auth      | Notes                                              |
|----------------------------------------------|-----------|----------------------------------------------------|
| `POST /v1/onboard/register`                  | public    | Body: SEC1-uncompressed public key + key_id        |
| `POST /v1/onboard/challenge`                 | user      | Issues a short-lived nonce                         |
| `GET  /v1/me`                                | user      | The user's current status                          |
| `GET  /v1/events`                            | public    | Published, upcoming                                |
| `GET  /v1/events/{id}`                       | public    | 404 unless published                               |
| `POST /v1/admin/login`                       | public    | Body: email + password → bearer token              |
| `GET  /v1/admin/events`                      | admin     | Published + unpublished (event picker)             |
| `POST /v1/admin/events`                      | admin     | Create an event                                    |
| `GET  /v1/admin/events/{id}/verified-count`  | admin     | Live count for the scanner UI                      |
| `POST /v1/admin/verify`                      | admin     | The scan: `{nonce, signature_b64, event_id}` → 200 |
| `GET  /admin`                                | public    | Static admin tool (HTML)                           |
| `GET  /v1/search?q=…`                        | public    | Brave Search proxy — powers the marketing search bar |
| `GET  /health`                               | public    | Liveness — returns `"ok"` if the process is up     |
| `GET  /`                                     | public    | Marketing site (`web/index.html`), and fallback for any unmatched path |

Auth is `Authorization: Bearer <token>`. Only `sha256(token)` is
persisted server-side.

## Admin tool

Open `http://localhost:3000/admin` (in dev) or `https://your-host/admin`
in prod. Sign in with the seeded admin, pick an event, point the
camera at a user's QR. The scanner uses the browser's `BarcodeDetector`
— available in Chrome / Edge / Safari 17+. Firefox falls back to a
manual-paste textarea.

**Camera access requires HTTPS or `localhost`.** In production, the
admin tool must be served behind a TLS terminator (nginx / Caddy /
similar). Without HTTPS, `navigator.mediaDevices.getUserMedia` refuses
to grant camera permission.

## Security boundary

- **Two roles, on purpose.** The migration role owns objects and can
  bypass RLS; the runtime role (`bf_app`) cannot. They are NOT the
  same database user. `DATABASE_URL` uses `bf_app`;
  `MIGRATION_DATABASE_URL` is owner-only.
- **RLS scoping via transaction-local GUCs.** Every request opens
  either an `RlsTransaction` (user-scoped) or `AdminRlsTransaction`
  (admin-scoped). The GUC is always `is_local = true` so it cannot
  leak across pool connections.
- **Verification is in-person.** Pending users sit on a 30-day TTL
  unless an admin scans their QR at a real event. No remote
  self-verification path exists.
- **P-256 ECDSA**, Apple Secure Enclave compatible: SEC1 uncompressed
  keys, DER signatures, SHA-256 prehash, low-S normalised on verify.
- **Audit out of transaction.** The audit trail survives a rollback,
  and the `audit_events` table is append-only (no UPDATE/DELETE
  grant + trigger).

## Tests

```bash
DATABASE_URL='postgresql://bf_app:bf_app@localhost:5434/box_fraise' \
  cargo test --test integration
```

12 tests, one ignored (the iOS-fixture slot — swap in a real
on-device capture when convenient). The rest cover the RLS
invariants, the verify race, replay rejection, expired challenges,
tampered signatures, audit append-only, and the two-role enforcement.

## Layout

```
src/
  main.rs              — bin shim
  lib.rs               — module roots
  app.rs               — AppState, router, seed-admin bootstrap
  config.rs            — env loading
  db.rs                — RlsTransaction, AdminRlsTransaction
  crypto.rs            — P-256 verify, token gen, Argon2
  audit.rs             — append-only audit write
  error.rs             — AppError + IntoResponse
  maintenance.rs       — periodic prune
  http/                — middleware + extractors + admin asset serve
  domain/admin/        — admin login
  domain/onboarding/   — register, challenge, verify, me
  domain/events/       — public list/get + admin list/create/count
migrations/
  0001_init.sql        — schema, RLS, policies, grants
admin/index.html       — single-file admin tool
docker-compose.yml     — Postgres (init script creates bf_app)
docker/init/01-roles.sql
```
