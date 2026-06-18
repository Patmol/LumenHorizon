# syntax=docker/dockerfile:1

ARG RUST_VERSION=1.95.0

FROM rust:${RUST_VERSION}-bookworm AS builder

WORKDIR /workspace

COPY backend ./backend

WORKDIR /workspace/backend

RUN cargo build --release -p db-migrate

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libgnutls30 \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system db-migrate \
    && useradd --system \
        --gid db-migrate \
        --home-dir /app \
        --shell /usr/sbin/nologin \
        db-migrate

WORKDIR /app

COPY --from=builder /workspace/backend/target/release/db-migrate /usr/local/bin/db-migrate

RUN chown -R db-migrate:db-migrate /app /usr/local/bin/db-migrate

USER db-migrate

ENTRYPOINT ["/usr/local/bin/db-migrate"]