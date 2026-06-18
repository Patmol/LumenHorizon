# syntax=docker/dockerfile:1

ARG RUST_VERSION=1.95.0

FROM rust:${RUST_VERSION}-bookworm AS builder

WORKDIR /workspace

COPY backend ./backend

WORKDIR /workspace/backend

RUN cargo build --release -p processing-svc

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        gdal-bin \
        libgnutls30 \
        libgdal32 \
        libhdf5-103-1 \
        libproj25 \
        proj-bin \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system processing-svc \
    && useradd --system \
        --gid processing-svc \
        --home-dir /app \
        --shell /usr/sbin/nologin \
        processing-svc

WORKDIR /app

COPY --from=builder /workspace/backend/target/release/processing-svc /usr/local/bin/processing-svc

RUN chown -R processing-svc:processing-svc /app /usr/local/bin/processing-svc

USER processing-svc

ENTRYPOINT ["/usr/local/bin/processing-svc"]
CMD ["worker"]