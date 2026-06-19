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
