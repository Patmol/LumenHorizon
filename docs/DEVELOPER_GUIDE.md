# Developer Guide

This guide gets a clean clone running with the local LumenHorizon backend stack. It documents active local commands only; remote hosting workflows are not part of the current repository scope.

## Prerequisites

- Rust from [backend/rust-toolchain.toml](../backend/rust-toolchain.toml)
- Docker or a compatible container runtime
- `just`
- GDAL command-line tools for processing flows
- Optional: a storage inspection tool for local Azurite blobs and queues

## First Run

```bash
just setup
just up
just migrate
just serve
```

`just setup` creates `.env` when needed and validates local tool availability. `just up` starts PostgreSQL and Azurite from [compose.yml](../compose.yml). `just migrate` applies database migrations. `just serve` starts `ingest-svc`.

Start the API Gateway separately:

```bash
just serve-api
```

## Command Reference

| Command | Purpose |
| --- | --- |
| `just up` | Start PostgreSQL and Azurite. |
| `just down` | Stop local dependencies. |
| `just migrate` | Apply local migrations. |
| `just serve` | Start `ingest-svc`. |
| `just serve-api` | Start `api-gateway`. |
| `just ingest daily` | Run one daily ingest pass. |
| `just ingest monthly` | Run one monthly ingest pass. |
| `just recover-ingest` | Recover downloaded/validated ingest rows and pending enqueue outbox records. |
| `just replay-rejected <ingest-id>` | Replay a rejected ingest row. |
| `just retention-cleanup` | Preview retention cleanup selections. |
| `just retention-cleanup-execute` | Execute retention cleanup. |
| `just openapi-check` | Verify implemented `/api/v1` routes against OpenAPI. |
| `just validate` | Run local API contract, Rust, test, lint, and Docker build checks. |

## Local API Gateway

`api-gateway` runs locally without every backed dependency configured, which keeps route hardening, request IDs, sanitized error envelopes, JWT/JWKS auth behavior, rate limiting, and OpenAPI checks easy to develop. When local dependencies are configured, it also serves tile manifests, tile redirects, tile-set listings, admin run lists, ingest triggers, and processing requeue.

Important local variables from [.env.example](../.env.example):

| Variable | Default | Purpose |
| --- | --- | --- |
| `API_GATEWAY_PORT` | `8080` | Local gateway listener port. |
| `API_GATEWAY_RUST_LOG` | `api_gateway=info` | Gateway tracing filter. |
| `RUNTIME_ENVIRONMENT` | `local` | Runtime profile for local feature gates. |
| `JWT_ISSUER` | local placeholder | Admin JWT issuer accepted by the gateway. |
| `JWT_AUDIENCE` | local placeholder | Admin JWT audience. |
| `JWKS_URL` | local placeholder | JWKS endpoint used for admin token validation. |
| `DATABASE_URL` | local PostgreSQL URL | Database-backed routes and run lists. |
| `AZURE_STORAGE_ACCOUNT` | `devstoreaccount1` | Azurite-compatible storage account. |
| `AZURE_STORAGE_ACCESS_KEY` | Azurite dev key | Storage key for local blobs and queues. |
| `AZURE_STORAGE_EMULATOR_HOST` | `127.0.0.1` | Azurite host. |
| `AZURE_QUEUE_NAME` | `viirs-processing` | Processing queue name. |
| `PROCESSED_TILES_CONTAINER` | `processed-tiles` | Processed tile artifact and manifest container. |
| `TILE_CDN_BASE_URL` | Azurite blob URL | Base URL stamped into newly generated tile manifests for direct tile loading. |
| `RATE_LIMIT_BACKEND` | `memory` | Local in-memory rate-limit backend; use `redis` with `REDIS_URL` for distributed profiles. |
| `MAX_URL_LENGTH_BYTES` | `8192` | Gateway URL length limit. |
| `ADMIN_MAX_BODY_BYTES` | `65536` | Gateway admin write body limit. |
| `PUBLIC_ROUTE_TIMEOUT_SECONDS` | `5` | Public route timeout. |
| `ADMIN_ROUTE_TIMEOUT_SECONDS` | `15` | Admin route timeout. |
| `HEALTH_ROUTE_TIMEOUT_SECONDS` | `2` | Health/readiness timeout. |

The local `TILE_CDN_BASE_URL` in [.env.example](../.env.example) points at Azurite's anonymous blob endpoint for `processed-tiles`. That keeps the native app on the production-style manifest contract while making local tile URLs reachable from the simulator and macOS app. Existing `.env` files are not rewritten by `just setup`; update or recreate `.env` before regenerating manifests if it still points at `https://tiles.lumenhorizon.com`.

## Local Tile Smoke

Local tile URLs are baked into each generated manifest. After changing `TILE_CDN_BASE_URL`, regenerate a tile set so the latest manifest carries the new base URL.

```bash
just up
just migrate
```

Start the HTTP services in separate terminals:

```bash
just serve
```

```bash
just serve-api
```

In another terminal, ingest or recover a queue message, then process one visible message:

```bash
cd backend
set -a && source ../.env && set +a
cargo run -p processing-svc -- process-once
```

Confirm the manifest and one substituted tile URL are reachable without storage auth headers:

```bash
curl -s http://127.0.0.1:8080/api/v1/tiles/manifest | jq '.data.tile_url_template'
curl -s "http://127.0.0.1:10000/devstoreaccount1/processed-tiles/tiles/<tile-set-id>/<z>/<x>/<y>.png" -o tile.png
file tile.png
```

`file tile.png` should report PNG image data. If the manifest still advertises `tiles.lumenhorizon.com`, update `.env` and process a new tile set; previously generated manifests keep their original URL template.

## Local Smoke Evidence

Run retention cleanup evidence after local dependencies and migrations are ready:

```bash
just up
just migrate
just retention-cleanup
just retention-cleanup-execute
```

Dry-run output should show selected stale raw or tile-set targets without deletion actions. Execute output should record deleted or missing events and mark eligible non-latest tile sets with retention metadata while preserving the latest plus protected prior tile sets. Use local data from ingest/processing runs or throwaway seeded data when practicing this flow.

Exercise anonymous API client behavior while the API Gateway is serving and a latest tile manifest is available:

```bash
just serve-api
curl --fail "$LUMENHORIZON_API_URL/api/v1/tiles/manifest"
curl --fail "$LUMENHORIZON_API_URL/api/v1/tiles/sets?limit=5"
curl -i "$LUMENHORIZON_API_URL/api/v1/tiles/<tile-set-id>/<z>/<x>/<y>.png"
```

The API guide shows a fuller manifest, pagination, and tile redirect workflow.

## Storage Inspection

Azurite exposes Blob and Queue endpoints locally. Use your preferred storage tool to inspect:

- `raw-viirs` for downloaded granules
- `processed-tiles` for generated tile artifacts and manifests
- `viirs-processing` for pending processing messages
- `viirs-processing-deadletter` for failed messages

## Validation

For the full active local validation stack:

```bash
just validate
```

Focused backend checks:

```bash
cd backend
cargo fmt --all -- --check
cargo check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Focused API Gateway checks:

```bash
cd backend
CARGO_TARGET_DIR=target/copilot-api-gateway cargo check -p api-gateway
CARGO_TARGET_DIR=target/copilot-api-gateway cargo test -p api-gateway
```

## Troubleshooting

- If services cannot connect to PostgreSQL or Azurite, run `just up` and confirm `.env` exists.
- If migrations fail, run `just down`, `just up`, then `just migrate` again and inspect the PostgreSQL container logs.
- If processing finds no queue message, confirm ingest reached `enqueued` status and inspect `viirs-processing` plus `ingest_recovery_outbox`.
- If tile routes return unavailable or not found responses, confirm processing has published a manifest under the processed tiles container.
