# Status Overview

## Current Snapshot

The repository is now a local/backend product codebase. The active implementation includes ingest, database migration, processing, shared contracts, tile generation, API Gateway routes, local developer scripts, Docker image builds, OpenAPI contract checks, and CI validation. Remote hosting resources and automation have been removed from active source and plans.

## Already Done

- Product/backend architecture and planning docs are active under `plans/`.
- Local PostgreSQL and Azurite are wired through [compose.yml](../../../compose.yml) and `just` commands.
- `ingest-svc` supports `serve`, `ingest`, `recover-ingest`, and `replay-rejected` workflows.
- `db-migrate` applies SQLx migrations for backend schemas.
- `processing-svc` supports queue processing, science processing, tile generation, manifests, and retention cleanup.
- `api-gateway` exposes `/api/v1` product/admin routes with request IDs, security headers, hardening, JWT/JWKS auth, route-class rate limiting, admin audit, and OpenAPI checks.
- CI runs Rust checks/tests, OpenAPI contract verification, Docker image builds, and image scans with generic local image names.

## Active Command Surface

- `just setup`
- `just up`
- `just down`
- `just migrate`
- `just serve`
- `just serve-api`
- `just ingest daily|monthly`
- `just recover-ingest`
- `just replay-rejected <ingest-id>`
- `just retention-cleanup`
- `just retention-cleanup-execute`
- `just openapi-check`
- `just validate`

## Current Roadmap Position

The active roadmap is continuous from Chunk 0 through Chunk 10 in [../00-implementation-roadmap.md](../00-implementation-roadmap.md). Chunks now represent product/backend work only:

- Chunks 0-5: foundation, local dependencies, ingest, quality gates, and CI.
- Chunks 6-8: processing, science products, tiles, and retention.
- Chunks 9-10: API Gateway, client contracts, runbooks, deferred site-route placeholders, and local launch-readiness cleanup.

## Remaining Work

See [gap-register.md](gap-register.md). Remaining gaps are product/backend gaps: deeper fixtures, data validation, API/client smoke evidence, and documentation polish.
