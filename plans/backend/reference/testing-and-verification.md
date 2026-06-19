# Testing And Verification

## Local Full Stack

```bash
just validate
```

`just validate` runs the OpenAPI contract check, Rust formatting/check/lint/test coverage, and local Docker image builds.

## Rust Checks

```bash
cd backend
cargo fmt --all -- --check
cargo check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## API Contract

```bash
just openapi-check
```

The check verifies implemented `/api/v1` gateway routes against [backend/api-gateway/openapi/openapi.yaml](../../../backend/api-gateway/openapi/openapi.yaml), including route inventory and auth posture.

## Script Checks

```bash
bash -n scripts/*.sh
```

## Local Smoke Evidence

Retention cleanup evidence uses local PostgreSQL metadata and Azurite blobs:

```bash
just up
just migrate
just retention-cleanup
just retention-cleanup-execute
```

Dry-run evidence should show selected raw and tile-set targets without deletion actions. Execute evidence should record `deleted` or `missing` events and mark eligible non-latest tile sets with retention metadata while preserving latest plus protected prior tile sets. Use local ingest/processing data or throwaway seeded rows when practicing cleanup behavior.

API client smoke evidence uses anonymous product routes while the API Gateway is serving and a latest tile manifest is available:

```bash
just serve-api
curl --fail "$LUMENHORIZON_API_URL/api/v1/tiles/manifest"
curl --fail "$LUMENHORIZON_API_URL/api/v1/tiles/sets?limit=5"
curl -i "$LUMENHORIZON_API_URL/api/v1/tiles/<tile-set-id>/<z>/<x>/<y>.png"
```

The smoke flow fetches the latest manifest, follows tile-set pagination through `meta.next_cursor`, and verifies a tile redirect returns `302` with a substituted `Location` URL.

## Docker Checks

```bash
just docker-build
just docker-build-db-migrate
just docker-build-processing
just docker-build-api-gateway
```

CI also scans built images for HIGH and CRITICAL vulnerabilities.

## Focus Areas

- Ingest discovery parsing, pagination, storage paths, queue payloads, idempotency, recovery, and replay.
- Processing queue receive/delete/retry/dead-letter behavior.
- Science dataset mapping, quality filtering, classification evidence, and rejection reasons.
- Tile math, manifest shape, latest pointer publication, and cleanup selection.
- API Gateway request hardening, auth, rate limits, route envelopes, OpenAPI alignment, and admin policies.

## Evidence Standard

A completed chunk should state which checks ran and whether any skipped checks require follow-up. Tests that need local services should document the required setup.
