# LumenHorizon Backend

This Rust workspace contains the backend services and shared support crates for local LumenHorizon development.

## Crates

- `api-gateway` - Axum API surface for product and admin routes under `/api/v1`.
- `db-migrate` - SQLx migration runner for local PostgreSQL schemas.
- `ingest-svc` - CMR discovery, Earthdata download, raw blob persistence, queue emission, ingest recovery, and rejected-row replay.
- `processing-svc` - Queue worker, science processing, tile generation, manifest publication, and retention cleanup.
- `shared` - narrow shared contracts and low-level helpers used across backend services.

## Shared Boundaries

The `shared` crate owns code that is identical across services: PostgreSQL pool construction, JSON tracing initialization, HTTP retry policy, Slippy Map tile math, processing-message contracts, verified product metadata, and Blob/Queue REST signing primitives. Service-owned behavior stays in the owning service crate: configuration structs, database queries, queue loops, retries, recovery policy, admin route behavior, and processing workflow decisions.

Storage access intentionally uses small local helpers instead of a broad storage SDK. The current runtime targets Azurite locally and Blob/Queue REST-compatible APIs in code, which keeps local development deterministic while leaving future storage replacement as a separate design decision.

## Common Commands

Run from the repository root unless noted:

```bash
just up
just migrate
just serve
just serve-api
just check
just openapi-check
```

Focused backend checks:

```bash
cd backend
cargo fmt --all -- --check
cargo check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Configuration

Local configuration lives in `.env` at the repository root. The services expect PostgreSQL and Azurite-compatible storage variables such as `DATABASE_URL`, `AZURE_STORAGE_ACCOUNT`, `AZURE_STORAGE_ACCESS_KEY`, and `AZURE_STORAGE_EMULATOR_HOST`.
