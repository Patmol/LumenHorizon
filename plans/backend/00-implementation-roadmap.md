# 00. Implementation Roadmap

This roadmap is renumbered after removing remote hosting work from the active project. Chunks now cover product behavior, local backend services, data processing, API contracts, and local verification only.

## Chunk Rules

- A chunk must be independently reviewable and have clear verification.
- Runtime storage remains Azurite/Blob/Queue REST-compatible until a separate storage replacement design exists.
- Local developer commands and CI must stay aligned.
- API and database contract changes must update docs and tests in the same chunk.

## Chunk 0 - Product And Architecture Baseline

Purpose: define the product promise, target data products, backend service boundaries, and local-first architecture.

Deliverables:

- Product goals and non-goals.
- Backend crate/service map.
- Local dependency model with PostgreSQL and Azurite.
- Initial reference docs and status tracking.

Verification:

- Plan links resolve from [overview.md](overview.md).
- Architecture docs match repository layout.

## Chunk 1 - Rust Workspace And Configuration

Purpose: create stable backend crate boundaries and environment-driven configuration.

Deliverables:

- Rust workspace with `ingest-svc`, `db-migrate`, and `shared`.
- Service commands for serve, ingest, and migrations.
- `.env.example` and local configuration validation.
- Structured logging and shared PostgreSQL pool helper.

Verification:

- `cargo check` passes in `backend/`.
- Missing/invalid config produces sanitized errors.

## Chunk 2 - Local Dependencies And Database Foundation

Purpose: make local development reproducible.

Deliverables:

- Compose-backed PostgreSQL and Azurite.
- Local scripts for setup, start, stop, migration, and service execution.
- Initial SQLx migrations for ingest metadata.
- Local Docker image foundation.

Verification:

- `just setup`, `just up`, `just migrate`, and `just serve` work from a clean clone.
- `just docker-build` builds the ingest image.

## Chunk 3 - Ingest MVP

Purpose: ingest VIIRS granules from discovery through queue emission.

Deliverables:

- CMR discovery for selected products.
- Earthdata download boundary.
- Raw blob upload and relative blob path persistence.
- Processing queue message contract.
- Basic health and readiness routes.

Verification:

- Unit tests cover discovery parsing, storage paths, queue payloads, and config.
- A controlled local ingest can persist metadata and enqueue work.

## Chunk 4 - Ingest Hardening

Purpose: make ingest deterministic, resumable, and debuggable.

Deliverables:

- Idempotency for duplicate logical ingest rows.
- CMR pagination and resume logging.
- Error classification and compatibility checks.
- Durable enqueue recovery outbox.
- Rejected-row replay command.

Verification:

- SQLx and unit tests cover duplicate rows, status transitions, recovery, and replay.
- `just recover-ingest` and `just replay-rejected <ingest-id>` are documented.

## Chunk 5 - Local Quality Gates And CI

Purpose: keep local and CI checks aligned.

Deliverables:

- Rust formatting, check, clippy, and workspace tests.
- API Gateway OpenAPI contract test.
- Docker builds and image vulnerability scans in CI.
- `just validate` local equivalent.

Verification:

- `just validate` runs without cloud tools or remote credentials.
- CI uses generic local image names and does not push images.

## Chunk 6 - Processing Service Foundation

Purpose: add queue-driven processing service behavior.

Deliverables:

- `processing-svc` workspace member.
- `worker`, `process-once`, and `process-message <json>` command boundaries.
- Queue receive/delete/retry/dead-letter behavior.
- Shared processing message contracts.
- `processing_log` migration and idempotent start handling.

Verification:

- Unit tests cover queue behavior, malformed messages, parsed failures, and idempotency.
- Local processing can consume a queued message.

## Chunk 7 - Science Processing And Multi-Product Ingest

Purpose: turn raw VIIRS granules into classified processing evidence.

Deliverables:

- Verified VIIRS Black Marble dataset mappings.
- GDAL CLI boundary for HDF-EOS5 reads.
- Quality/cloud filtering and sampled summaries.
- Versioned radiance dark-sky classification evidence.
- Product/cadence metadata for daily and monthly ingest.

Verification:

- Tests cover dataset mapping, quality logic, product/cadence selection, and processing summaries.
- Local smoke can process a representative raw blob when fixtures/dependencies are present.

## Chunk 8 - Tile Generation And Retention

Purpose: publish durable map tile artifacts and manage local retention policy.

Deliverables:

- Slippy Map tile math and bounds clipping.
- PNG tile generation and transparent nodata/rejected rendering.
- Immutable tile manifests and `latest` pointer publication.
- `tile_sets` metadata and processing output references.
- Retention cleanup dry-run and explicit execute modes.
- Product claim policy for `radiance-dark-sky-v1`.

Verification:

- Tests cover tile math, bounds, manifest shape, latest promotion, and cleanup selection.
- `just retention-cleanup` previews deletions without removing blobs.

## Chunk 9 - API Gateway And Client Contracts

Purpose: expose the backend through stable local API contracts.

Deliverables:

- API Gateway `/health`, `/ready`, product routes, and admin routes.
- Request IDs, security headers, disabled CORS default, sanitized errors, and request hardening.
- RS256/JWKS admin JWT validation with route-to-role policy coverage.
- Route-class rate limiting with memory and Redis-compatible stores.
- Tile manifest, tile-set, tile-class, and tile redirect APIs.
- OpenAPI route/auth contract checks.

Verification:

- `just openapi-check` passes.
- Gateway tests cover auth, rate limits, hardening, tile routes, and admin route policies.

## Chunk 10 - Local Launch Readiness

Purpose: close product/backend gaps before any future hosting design begins.

Deliverables:

- Status and gap register reflect local backend reality.
- Runbooks cover local database restore practice, queue backlog investigation, and retention cleanup.
- API docs and developer docs match current commands.
- Final search confirms no orphaned removed hosting references remain.

Verification:

- Full local validation passes.
- Docs contain no links to deleted files or removed workflows.
- Remaining gaps are product/backend gaps, not hosting automation gaps.
