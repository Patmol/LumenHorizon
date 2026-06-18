# LumenHorizon

LumenHorizon is a cross-platform stargazing product backed by Rust services for VIIRS Black Marble ingest, processing, tile generation, and API access. The repository now focuses on local development, backend behavior, API contracts, and product/data processing foundations. Remote hosting automation is intentionally out of scope for the active codebase.

## Current Scope

The active backend includes:

- `ingest-svc` for CMR discovery, Earthdata downloads, raw blob persistence, processing queue emission, recovery, and rejected-row replay.
- `processing-svc` for queue-driven science processing, GDAL-backed VIIRS reads, quality filtering, dark-sky classification evidence, tile generation, manifest publication, and retention cleanup.
- `api-gateway` for `/api/v1` product/admin routes, request IDs, security headers, request hardening, JWT/JWKS admin auth, route-class rate limiting, OpenAPI contract checks, tile metadata, tile redirects, admin run lists, ingest triggers, and processing requeue.
- `db-migrate` for local SQLx migration execution.
- `shared` for narrow reusable contracts and low-level helpers shared across backend services.

Local PostgreSQL and Azurite provide the default development dependencies. The storage code uses Blob and Queue REST-compatible semantics because Azurite is the local emulator for those APIs.

## Repository Layout

```text
backend/    # Rust workspace: api-gateway, db-migrate, ingest-svc, processing-svc, shared
docs/       # Developer and API guides
plans/      # Product/backend roadmap, reference docs, status, and local runbooks
scripts/    # Local developer helpers
compose.yml # Local PostgreSQL and Azurite services
justfile    # Local task runner
```

## Prerequisites

- Rust toolchain from [backend/rust-toolchain.toml](backend/rust-toolchain.toml)
- Docker or a compatible container runtime
- `just`
- GDAL CLI tools for processing tests and local processing flows
- Optional: a storage inspection tool for local Azurite blobs and queues

## Local Development

From the repository root:

```bash
just setup
just up
just migrate
just serve
```

Useful commands:

```bash
just serve-api
just ingest daily
just ingest monthly
just recover-ingest
just retention-cleanup
just openapi-check
just validate
```

`just validate` runs local API contract checks, Rust formatting/check/lint/test coverage, and local Docker image builds. It does not run remote hosting or cloud validation.

## Docker

Local image builds are available for each backend binary:

```bash
just docker-build
just docker-build-db-migrate
just docker-build-processing
just docker-build-api-gateway
```

CI builds and scans the same images with generic `lumenhorizon/*:<commit-sha>` tags. Images are not pushed by the active workflows.

## Documentation

- [docs/DEVELOPER_GUIDE.md](docs/DEVELOPER_GUIDE.md) covers local setup, commands, troubleshooting, and validation.
- [docs/API_GUIDE.md](docs/API_GUIDE.md) covers the current HTTP API behavior and examples.
- [plans/overview.md](plans/overview.md) links the active planning documents.

## Configuration

Start from [.env.example](.env.example). Local defaults target PostgreSQL and Azurite. Secrets should stay in local environment files or protected developer environments; do not commit real credentials.
