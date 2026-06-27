# Investigation Handoff: Latest Overlay Covers Only Part Of The US

## Status

Open. Targeted handoff for the remaining backend/data coverage issue only.
The app-side overzoom issue is out of scope for this document.

## Problem

After the visible processing queue is drained, the latest overlay still covers
only a small part of the United States. The currently observed latest coverage
is an East Coast rectangle, not broad US coverage.

This is not just a missing-blob question. The fixer should determine and then
change what `latest` means for the product: a complete product/date mosaic, a
partitioned latest per product/cadence, or an explicitly labeled per-granule
view.

## Root Cause Summary

The current backend does not publish a US mosaic. It publishes one tile set per
processed VIIRS granule and promotes every successful tile set as the single
global `latest`.

As a result, draining the processing queue leaves `latest` pointing at whichever
individual granule completed most recently. It does not point at the union of
all processed queue items.

## Live Evidence From Local Runtime

Public API evidence gathered on 2026-06-23:

- Latest manifest endpoint: `http://127.0.0.1:8080/api/v1/tiles/manifest`
- Latest manifest observed:
  - `tile_set_id`: `2026-05-01-radiance-dark-sky-v1-f61ab242-a1`
  - `dataset_date`: `2026-05-01`
  - `source_granules`: one entry
  - `product/source`: `VNP46A3/2026-05-01/h10v05.h5`
  - `bounds`: west `-80.15625`, south `29.84064389983442`, east `-69.9609375`, north `40.17887331434696`
  - `tile_count`: `1449`
  - `min_zoom`: `3`, `max_native_zoom`: `10`, `max_display_zoom`: `12`
- Tile set list endpoint: `http://127.0.0.1:8080/api/v1/tiles/sets?limit=1&cursor=N`
- Sample tile-set rows show other processed coverage exists but is not part of latest:
  - cursor `0`: `2026-05-01-radiance-dark-sky-v1-f61ab242-a1`, `latest=true`, East Coast-ish bounds, `tile_count=1449`, `created_at=2026-06-23T03:21:54.882021Z`
  - cursor `1`: `2026-06-09-radiance-dark-sky-v1-b76a75f0-a1`, `latest=false`, West Coast bounds west `-125.15625` east `-119.8828125`, `tile_count=300`
  - cursor `2`: `2026-06-09-radiance-dark-sky-v1-26981e80-a1`, `latest=false`, southeast/eastern edge bounds west `-70.3125` east `-65.7421875`, `tile_count=269`
  - cursor `5`: `2026-06-07-radiance-dark-sky-v1-d75c6b05-a1`, `latest=false`, West Coast bounds, `tile_count=122`
  - cursor `10`: `2026-06-05-radiance-dark-sky-v1-96a16317-a1`, `latest=false`, West Coast bounds, `tile_count=193`

Important implication: `latest` is not equivalent to newest dataset date. The
single global latest pointer can allow a monthly `VNP46A3` tile set dated
`2026-05-01` to supersede daily `VNP46A2` rows dated `2026-06-09` simply because
it was promoted later.

## Code Evidence

### One Queue Message Represents One Granule

- `backend/shared/src/processing_message.rs` defines `ProcessingMessage` with one
  `ingest_id`, one `blob_path`, one `product`, one `granule_date`, one `tile_h`,
  and one `tile_v`.
- `backend/ingest-svc/src/models.rs` builds one processing message from one
  discovered `GranuleCandidate`.
- `backend/ingest-svc/src/jobs/pipeline.rs` enqueues that one message to
  `viirs-processing`.

### One Message Becomes One Tile Set

- `backend/processing-svc/src/process/message.rs` builds
  `source_granules = vec![source_granule_for_message(processing_message)]`.
- `backend/processing-svc/src/process/message.rs` calls
  `generate_tile_set_for_granule_with_manifest(...)` with that one-source vector.
- `backend/processing-svc/src/process/message.rs` builds tile set IDs from
  `granule_date`, classification version, ingest-id prefix, and attempt. The ID
  is attempt/granule scoped, not mosaic scoped.

### Generation Is Single-Granule Scoped

- `backend/processing-svc/src/generate/orchestration.rs` describes tile-set
  orchestration for one source granule.
- `GranuleTileSetRequest` carries one `granule_path`, one source-raster extent,
  and source metadata.
- Generation clips the source granule bounds to configured bounds, plans tiles
  for that clipped extent, filters empty tiles, then derives manifest `bounds`
  from rendered tiles in that one tile set.

### Publishing Promotes Every Successful Tile Set As Global Latest

- `backend/processing-svc/src/process/message.rs` calls
  `publish_generated_tile_set(...)` for each processed message.
- `backend/processing-svc/src/publish.rs` inserts the tile set row and calls
  `db::promote_latest_tile_set(pool, &manifest.tile_set_id)`.
- `backend/processing-svc/src/publish.rs` uploads `manifests/latest.json`
  pointing to the just-published manifest.
- `backend/processing-svc/src/db.rs` demotes all current latest rows, then sets
  the requested tile set `latest=true`.

### Schema Enforces One Global Latest Pointer

- `backend/db-migrate/migrations/004_create_tile_sets.sql` stores tile set
  metadata but has no product, cadence, or mosaic grouping column.
- `idx_tile_sets_latest_one` is a partial unique index on `latest=true`, so only
  one row can be latest globally.
- `backend/db-migrate/migrations/003_create_processing_log.sql` stores one
  processing row per `ingest_id` and one optional `tile_set_id`.

### API Latest Follows The Moving Blob Pointer

- `backend/api-gateway/src/server/routes.rs` serves latest manifest through
  `storage.latest_manifest()`.
- `backend/api-gateway/src/storage.rs` reads `manifests/latest.json` and then
  reads the pointed manifest blob.

## Historical Secondary Contributor

At the time of this investigation, local development could be capped before
processing via `INGEST_MAX_GRANULES`, and `backend/ingest-svc/src/jobs.rs`
applied that cap to discovered granules. That cap has since been removed from
source configuration; use a narrower `BOUNDING_BOX` for small local smoke runs
instead. Confirm current ingest code and local `.env` before assuming all
expected US granules were downloaded, validated, and enqueued.

## Non-Root Causes Already Ruled Out

- Not simply “no processed tiles exist outside the East Coast”: tile-set list
  samples show separate West Coast tile sets exist.
- Not a latest API sorting issue by dataset date: the API reads a blob pointer,
  and the DB has a single `latest=true` row.
- Not a manifest bounds parsing issue: latest bounds match one h/v tile and
  rendered non-empty native tiles, not a continental mosaic.

## Direct SQL/Azurite Evidence To Gather Before Fixing

Run these against the local Postgres and Azurite state before implementation so
the fix is grounded in current data distribution.

```sql
SELECT id, dataset_date, latest, tile_count, bounds, manifest_blob_path, created_at
FROM tile_sets
WHERE retention_deleted_at IS NULL
ORDER BY latest DESC, dataset_date DESC, created_at DESC, id DESC;

SELECT latest, count(*)
FROM tile_sets
WHERE retention_deleted_at IS NULL
GROUP BY latest;

SELECT status, count(*)
FROM processing_log
GROUP BY status
ORDER BY status;

SELECT product, tile_h, tile_v, status, count(*)
FROM processing_log
GROUP BY product, tile_h, tile_v, status
ORDER BY product, tile_h, tile_v, status;

SELECT p.product, p.tile_h, p.tile_v, p.status, p.tile_set_id,
       p.cloud_fraction, p.valid_pixel_count, p.rejected_pixel_count,
       p.error_message, p.updated_at
FROM processing_log p
ORDER BY p.updated_at DESC;

SELECT status, count(*)
FROM ingest_log
GROUP BY status
ORDER BY status;

SELECT product, tile_h, tile_v, status, count(*)
FROM ingest_log
GROUP BY product, tile_h, tile_v, status
ORDER BY product, tile_h, tile_v, status;
```

Inspect queue and blob state as well:

```bash
az storage message peek \
  --queue-name viirs-processing \
  --num-messages 32 \
  --connection-string "$AZURITE_CONNECTION_STRING"

az storage message peek \
  --queue-name viirs-processing-deadletter \
  --num-messages 32 \
  --connection-string "$AZURITE_CONNECTION_STRING"

az storage queue metadata show \
  --name viirs-processing \
  --connection-string "$AZURITE_CONNECTION_STRING"

az storage queue metadata show \
  --name viirs-processing-deadletter \
  --connection-string "$AZURITE_CONNECTION_STRING"

az storage blob list \
  --container-name processed-tiles \
  --prefix manifests/ \
  --connection-string "$AZURITE_CONNECTION_STRING" \
  --query "[].name" -o tsv

az storage blob list \
  --container-name processed-tiles \
  --prefix "tiles/<tile_set_id>/" \
  --connection-string "$AZURITE_CONNECTION_STRING" \
  --query "[].name" -o tsv | sort | head -50
```

## Fix Direction Decision

Pick one product contract before coding.

### Option A: Product/Date Mosaic Latest

Build one combined tile set per product/cadence/dataset date/classification
version from all accepted granules intersecting the target bounds. Promote only
that combined tile set as public latest.

This is the likely product fix if the map should show broad US coverage.

### Option B: Partitioned Latest Pointers

Track latest separately per product/cadence/dataset date or expose API filters
so daily and monthly products cannot overwrite each other globally.

This can be combined with Option A if the product needs one mosaic per product.

### Option C: Explicit Per-Granule Browsing

Keep the current publication model but rename/API-label it as per-granule. Do
not present this as broad US latest coverage.

This is the smallest backend behavior change but does not satisfy a US overlay
coverage expectation.

## Implementation Guardrails

- Do not continue promoting per-granule intermediates as public latest when a
  broad mosaic is expected.
- If mosaic mode is implemented, publish per-granule intermediates without
  latest promotion or write them to an internal staging namespace.
- Atomically publish the complete mosaic manifest and latest pointer only after
  all intended source granules have been included or deliberately excluded.
- Preserve rejection semantics for cloudy, invalid, out-of-bounds, and
  no-renderable-tile granules; mosaic generation should not silently hide these
  outcomes.
- Decide how daily `VNP46A2`/`VJ146A2` and monthly `VNP46A3` should coexist
  before changing the schema or API contract.

## Acceptance Criteria

1. After processing multiple accepted granules for a target product/date, the
   latest manifest `source_granules.length` is greater than `1` and matches the
   included granules.
2. Latest manifest `bounds` approximate the union of included non-empty coverage,
   not the h/v bounds of the last granule processed.
3. The tile-set list shows a deliberate latest row for the mosaic or the API
   clearly exposes that the selected latest is per product/date.
4. Per-granule intermediate publication cannot silently overwrite the public
   latest mosaic.
5. Regression tests cover latest publication semantics, including the case where
   a later per-granule processing result must not replace a complete mosaic.
6. Verification includes `tile_sets`, `processing_log`, `ingest_log`, queue,
   dead-letter, and processed blob evidence.

## Suggested Tests

- Unit-test `publish_generated_tile_set` or its replacement so an intermediate
  tile set can be inserted without moving `latest` when mosaic mode is expected.
- Add a SQL/integration test for the chosen latest partitioning model.
- Add generation/publish tests for a combined manifest whose `source_granules`,
  `bounds`, and `tile_count` come from multiple accepted granules.
- Keep existing processing rejection tests intact so rejected granules remain
  visible in `processing_log` rather than disappearing from coverage accounting.

## Key Files

- `backend/processing-svc/src/process/message.rs` - one message to one tile set
  and current latest publication call.
- `backend/processing-svc/src/generate/orchestration.rs` - single-granule tile
  generation and bounds derivation.
- `backend/processing-svc/src/publish.rs` - tile/manifest upload and latest
  pointer publication.
- `backend/processing-svc/src/db.rs` - `insert_tile_set`,
  `promote_latest_tile_set`, processing status updates.
- `backend/db-migrate/migrations/003_create_processing_log.sql` - one processing
  row per ingest and `tile_set_id` metadata.
- `backend/db-migrate/migrations/004_create_tile_sets.sql` - global single latest
  schema.
- `backend/api-gateway/src/storage.rs` - latest pointer resolution.
- `backend/api-gateway/src/server/routes.rs` - latest manifest and tile set list
  routes.
- `backend/ingest-svc/src/jobs.rs` - ingest discovery filtering and queue
  enqueue flow.
- `backend/ingest-svc/src/jobs/pipeline.rs` - one granule to one queue message.
- `backend/ingest-svc/src/cmr.rs` - broad CMR bounding-box discovery.
- `.env.example` - local ingest and tile bounds defaults.