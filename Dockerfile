# syntax=docker/dockerfile:1.7

# ── build stage ──────────────────────────────────────────────────────
# Rust slim image + system libs sqlx / rustls need at compile time.
# 1.85+ is required because some deps in the graph (e.g. base64ct 1.8+)
# use edition2024, which was only stabilised in that release. Bump if
# a future dep needs newer; do not downgrade.
FROM rust:1.95-slim-bookworm AS build

WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
        pkg-config libssl-dev ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy sources. All sqlx queries are the runtime `query(..)` form (not
# the compile-time `query!` macro), so no DATABASE_URL is needed at
# build.
#
# Only what the compiler actually reads belongs here. `admin` does —
# its HTML is baked into the binary with include_str!. `web` and
# `migrations` do NOT: both are read from disk at runtime, so they are
# copied straight into the runtime stage instead.
#
# That distinction is worth money. Anything copied above this RUN
# invalidates the layer cache and forces a full rebuild of the whole
# dependency graph — several minutes. With `web` here, every change to
# a paragraph of marketing copy recompiled axum, sqlx and p256 before
# it could ship.
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY admin ./admin

RUN cargo build --release --locked --bin box-fraise

# ── runtime stage ────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Binary + runtime assets.
#
# The admin tool's HTML is baked into the binary via include_str! —
# not copied here.
#
# The marketing site is served by `ServeDir("web")` at request time,
# so it MUST ship at /app/web (same relative path as in dev).
#
# Migrations are copied so an operator can `docker exec` psql against
# them; they are NOT applied automatically on boot.
# The site and the migrations come from the build context rather than
# the build stage — they were never needed to compile, and copying them
# here means changing either one skips the Rust build entirely.
COPY --from=build /app/target/release/box-fraise /usr/local/bin/box-fraise
COPY web /app/web
COPY migrations /app/migrations

# Railway (and most PaaS) inject $PORT; Config::from_env picks it up
# and constructs BIND_ADDR = 0.0.0.0:$PORT. BIND_ADDR wins if set
# explicitly.
EXPOSE 3000

CMD ["box-fraise"]
