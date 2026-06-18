# syntax=docker/dockerfile:1

ARG RUST_VERSION=1.95.0

FROM rust:${RUST_VERSION}-bookworm AS builder

WORKDIR /workspace

COPY backend ./backend

WORKDIR /workspace/backend

RUN cargo build --release -p api-gateway

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libgnutls30 \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system api-gateway \
    && useradd --system \
        --gid api-gateway \
        --home-dir /app \
        --shell /usr/sbin/nologin \
        api-gateway

WORKDIR /app

COPY --from=builder /workspace/backend/target/release/api-gateway /usr/local/bin/api-gateway

RUN chown -R api-gateway:api-gateway /app /usr/local/bin/api-gateway

USER api-gateway

ENTRYPOINT ["/usr/local/bin/api-gateway"]
CMD ["serve"]