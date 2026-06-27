# Global Overlay Coverage Scaling

Status: Future implementation plan. Validate behavior with the current US/CONUS-sized bounds first, but design each promoted chunk so the backend can scale to global VIIRS Black Marble coverage without changing product semantics later.

## Product Intent

LumenHorizon should eventually publish reliable global dark-sky overlays from NASA VIIRS Black Marble products while keeping the current local and US validation workflow practical. A user should be able to trust that a public latest mosaic represents a deliberate product/date coverage target, not whichever granule or partial batch happened to finish last.

The near-term goal is to prove the pipeline over the configured US bounds. The end goal is global coverage for selected products and cadences, with explicit capacity, quota, retention, monitoring, and failure semantics.

## Current Position

- NASA VIIRS Black Marble source products are global or near-global, but local configuration currently scopes ingest and tile generation with `BOUNDING_BOX=-125,24,-66,50` and `TILE_BOUNDS=-125,24,-66,50`.
- The backend now distinguishes per-granule tile sets from product/date mosaics, and public latest promotion is intended to represent the mosaic rather than a single granule.
- Full global coverage multiplies the number of source granules, rendered tiles, manifests, queue messages, processing attempts, and retained blobs by a large factor compared with the current US validation target.
- Global behavior needs parallelism and capacity controls before widening bounds. Changing bounds first would create long local runs, quota surprises, incomplete mosaics, and difficult-to-debug partial state.

## Proposed Scope

- Define coverage targets explicitly: US validation, larger regional rollout, and global rollout.
- Add a capacity model for each product/cadence/date that estimates expected VIIRS h/v granules, raw input bytes, processed tile count, blob count, queue depth, runtime, and retained storage.
- Make ingest concurrency configurable and bounded, including CMR page fetching, Earthdata downloads, raw blob uploads, database writes, and queue emission.
- Make processing concurrency configurable and bounded across worker instances and per-worker parallelism, while preserving idempotent processing, retries, visibility timeouts, and dead-letter behavior.
- Add product/date mosaic orchestration that can safely wait for, inspect, and publish complete target coverage at US scale first and global scale later.
- Add quota and retention guardrails so global jobs fail early or pause safely instead of partially publishing misleading latest overlays.
- Add operational evidence and runbooks for backlog, throughput, incomplete coverage, storage growth, retention cleanup, and recovery.

## Out Of Scope

- Widening production defaults to global coverage before US validation is reliable.
- Optimizing scientific classification beyond the existing dark-sky radiance classification contract.
- Replacing Azure Blob/Queue-compatible storage unless a separate storage replacement plan is promoted.
- User-facing region selection or client UI changes, except for exposing already-backed coverage metadata from tile manifests.
- Running global backfills by default in local development.

## Implementation Plan

### Phase 0 - Coverage Contract And Capacity Model

Define the contract before adding scale:

- Add a durable coverage-target model for `us`, `regional`, and `global`, including bounds, expected VIIRS h/v tiles, products, cadence, and dataset date.
- Keep the current US bounds as the default validation target.
- Document that global coverage is an explicit run mode, not a local default.
- Add a dry-run capacity estimator command that reports:
  - expected CMR granules by product/date/bounds,
  - expected in-bounds VIIRS h/v tiles,
  - estimated raw bytes,
  - estimated tile count/blob count by zoom range,
  - expected queue depth,
  - estimated processing time at configured concurrency,
  - estimated retained storage after retention policy.
- Make the estimator compare expected coverage against already-ingested and already-processed rows so operators can see remaining work before running a job.

Promotion candidates:

- `backend/shared` coverage-target and VIIRS-grid helpers.
- `ingest-svc` or `processing-svc` dry-run capacity command.
- Updates to `plans/backend/reference/configuration.md`, `plans/backend/reference/database-schema.md`, and operations docs.

### Phase 1 - US Validation Baseline

Prove correctness with the current US bounds before increasing scale:

- Run full US ingest for one monthly product/date, preferably `VNP46A3`, with no discovery truncation.
- Verify every expected in-bounds VIIRS h/v tile is either processed into the mosaic or explicitly rejected with a durable reason.
- Require public latest promotion to fail closed when the US mosaic is incomplete unless an explicit inspection-only override is provided.
- Record baseline runtime, queue depth, raw storage, processed storage, tile count, blob count, and failure/retry counts.
- Add regression tests for coverage completeness, expected h/v accounting, incomplete mosaic blocking, and product/date latest promotion.

Acceptance target:

- The latest US manifest has `tile_set_kind='mosaic'`, multiple `source_granules`, bounds that approximate configured US coverage, coverage metadata with no unexplained missing in-bounds tiles, and stable product/date latest behavior.

### Phase 2 - Parallel Ingest

Increase ingest throughput without losing determinism:

- Add bounded async concurrency for Earthdata downloads and raw blob uploads.
- Keep CMR paging deterministic and resumable; do not let concurrent pages reorder logical product/date coverage decisions in a way that makes results non-repeatable.
- Preserve idempotent ingest rows and queue emission recovery so repeated runs do not duplicate work.
- Add per-product/date/bounds ingest job identifiers or equivalent audit fields if current logs are not enough to reconstruct a run.
- Add configurable limits for:
  - maximum concurrent CMR requests,
  - maximum concurrent Earthdata downloads,
  - maximum concurrent raw uploads,
  - maximum queued messages per run or per product/date,
  - maximum raw bytes per run.
- Add rate-limit/backoff handling for NASA CMR and Earthdata failures. Fail visibly on sustained throttling or authentication failures.
- Expose ingest progress by expected h/v tile, not only by total row count.

Validation should still use US bounds first, then a larger regional target, before global.

### Phase 3 - Parallel Processing

Scale queue consumption while preserving safe publication:

- Support multiple processing workers and configurable per-worker parallelism.
- Revisit queue visibility timeout and dequeue limits for larger granules and higher zoom ranges.
- Ensure processing remains idempotent by `ingest_id`, product, date, classification version, render version, and attempt semantics.
- Keep per-granule intermediates internal or non-latest; only mosaic publication can promote public latest.
- Add throughput and failure metrics:
  - messages received/deleted/retried/dead-lettered,
  - processing duration by product/date/h/v,
  - tile render count and empty-tile count,
  - output bytes and blob count,
  - rejection reason counts.
- Add operator commands for pausing, resuming, replaying, and inspecting a product/date processing run.
- Verify that concurrent workers cannot race to publish incomplete or stale latest mosaics.

Validation should compare single-worker and multi-worker US output for equivalent manifest coverage and stable tile-set metadata.

### Phase 4 - Mosaic Publication At Scale

Make publication deliberate and resumable:

- Treat product/date mosaic publication as a separate orchestration step after ingest and processing evidence is available.
- Build coverage metadata from expected h/v tiles for the configured target.
- Publish mosaics atomically:
  - stage generated tiles and manifest,
  - validate source granule set and expected coverage,
  - insert mosaic tile-set metadata,
  - update product-latest pointer,
  - update global latest only when explicitly requested and allowed.
- Keep incomplete mosaics inspectable but not public latest by default.
- Add manifest fields or companion metadata that let clients and operators distinguish `us`, `regional`, and `global` coverage targets.
- Ensure API behavior remains backward compatible for existing latest manifest consumers.

### Phase 5 - Quota, Storage, And Retention Planning

Add guardrails before global runs:

- Define expected storage classes and retention windows for:
  - raw VIIRS blobs,
  - per-granule intermediate tiles,
  - product/date mosaic tiles,
  - manifests and latest pointers,
  - processing logs and audit rows.
- Add preflight quota checks for available blob capacity, queue capacity, database connection budget, and configured concurrency.
- Add budget thresholds that can block a run before downloads begin.
- Add cleanup policies for failed/staged outputs so global dry runs do not leave unbounded orphaned blobs.
- Revisit tile zoom policy for global coverage. A global first pass may need lower `TILE_MAX_NATIVE_ZOOM` or regional high-zoom publishing to control blob count.
- Document estimated storage and runtime for US, continent-sized, and global targets.

### Phase 6 - Global Rollout

Roll out by increasing coverage target size:

1. US validation target with default bounds.
2. One larger region or continent-sized bounds.
3. Global monthly `VNP46A3` for one dataset date.
4. Daily products for selected recent dates only after monthly global behavior is stable.
5. Routine global cadence only after monitoring, retention, and backfill controls are proven.

Each step should require an explicit capacity estimate, operator approval, successful ingest/processing completion, complete coverage accounting, and safe mosaic publication.

## Dependencies

- Existing product/date mosaic publication and coverage metadata.
- Current ingest idempotency, queue recovery, rejected-row replay, and processing retry/dead-letter behavior.
- Configuration docs for `BOUNDING_BOX`, `TILE_BOUNDS`, processing parallelism, cache headers, retention, and storage containers.
- Database schema support for distinguishing granule and mosaic tile sets, product latest pointers, and coverage metadata.
- Operational runbooks for queue backlog, retention cleanup, and failed run recovery.

## Open Questions And Comments

- What is the first global product: monthly `VNP46A3` only, or both monthly and daily products?
- Should global coverage include polar-adjacent VIIRS rows that have sparse/no usable Black Marble data, or should product-specific valid latitude limits be encoded in the coverage target?
- Should high native zoom be global, or should global coverage start at lower zoom with regional high-zoom refinement?
- What storage budget should block a global run?
- Do we need an explicit ingest/processing run table, or can existing `ingest_log`, `processing_log`, `tile_sets`, and manifest coverage metadata provide enough auditability?
- Should global latest coexist with US latest through named coverage targets, or should clients always request one public default?
- What operational environment will run global jobs: local developer machine, CI-like runner, scheduled worker, or managed batch service?

## Promotion Checklist

- US validation target completes with full expected coverage or explicit, durable rejection reasons.
- Capacity estimator exists and is documented before bounds are widened.
- Parallel ingest is bounded, resumable, idempotent, and tested against duplicate/retry paths.
- Parallel processing is bounded, idempotent, and safe under multiple workers.
- Incomplete mosaics cannot become public latest without an explicit override.
- Quota and storage preflight checks run before large regional or global jobs.
- Retention cleanup covers raw blobs, staged outputs, per-granule intermediates, mosaics, manifests, and metadata.
- API manifest coverage metadata clearly communicates the coverage target and completeness.
- Runbooks document US validation, regional rollout, global rollout, pause/resume, recovery, dead-letter handling, and cleanup.
- Documentation updates cover configuration, database schema, message/API contracts, observability, testing, and local development commands.
