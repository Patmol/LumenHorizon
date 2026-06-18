# syntax=docker/dockerfile:1

ARG RUST_VERSION=1.95.0

FROM rust:${RUST_VERSION}-bookworm AS builder

WORKDIR /workspace

COPY backend ./backend

WORKDIR /workspace/backend

RUN cargo build --release -p ingest-svc

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libgnutls30 \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system ingest-svc \
    && useradd --system \
        --gid ingest-svc \
        --home-dir /app \
        --shell /usr/sbin/nologin \
        ingest-svc

WORKDIR /app

COPY --from=builder /workspace/backend/target/release/ingest-svc /usr/local/bin/ingest-svc

RUN chown -R ingest-svc:ingest-svc /app /usr/local/bin/ingest-svc

USER ingest-svc

ENTRYPOINT ["/usr/local/bin/ingest-svc"]
CMD ["serve"]