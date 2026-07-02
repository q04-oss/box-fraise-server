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
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations
COPY admin ./admin
COPY web ./web

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
COPY --from=build /app/target/release/box-fraise /usr/local/bin/box-fraise
COPY --from=build /app/web /app/web
COPY --from=build /app/migrations /app/migrations

# Railway (and most PaaS) inject $PORT; Config::from_env picks it up
# and constructs BIND_ADDR = 0.0.0.0:$PORT. BIND_ADDR wins if set
# explicitly.
EXPOSE 3000

CMD ["box-fraise"]
