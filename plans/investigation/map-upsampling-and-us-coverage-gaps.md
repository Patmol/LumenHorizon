# Investigation: Missing Upsampling Tiles And Incomplete US Overlay Coverage

## Status

Open. Investigation handoff only; no code changes have been made for these
symptoms yet.

## Owner Handoff

This document is a self-contained task brief for the next agent. Investigate
both symptoms below, but keep the two root causes separate until evidence links
them:

1. **No overlay tiles are visible at zoom `>= 10`**, even though the manifest
   advertises a display range above native zoom.
2. **After processing all available queue items, only a small part of the US has
   visible overlay coverage**. The observed covered area is part of the East
   Coast, not broad US coverage.

Start by gathering runtime evidence from the local API, database, blob storage,
and app tile-request logs. Do not assume that an empty processing queue means the
latest published manifest represents a complete US mosaic.

## Severity

High. Missing upsampling makes the overlay disappear during normal map browsing,
and incomplete US coverage can make the latest tile set look like a product/data
failure even if the queue has been drained.

## User-Observed Symptoms

- The app shows no overlay tiles at zoom `>= 10`.
- The processing queue was drained / all visible queued items were processed.
- The latest overlay still covers only a small part of the United States,
  currently described as part of the East Coast.
- The expected investigation is not only "why are there missing tile blobs?" but
  also "why are we missing so much source data or published coverage?"

## Important Current-Code Context

### App zoom / upsampling path

- `app/LumenHorizon/LumenHorizon/Map/DarkSkyTileOverlay.swift`
  - `minimumZ = configuration.minZoom`
  - `maximumZ = configuration.maxNativeZoom`
  - `url(forTilePath:)` substitutes the path's `{z}/{x}/{y}` directly into the
    manifest template.
- `app/LumenHorizon/AppCore/Sources/AppCore/Tiles/TileOverlayConfiguration.swift`
  - `maxNativeZoom` and `maxDisplayZoom` are validated from the manifest.
  - `cameraZoomRange` allows the user to zoom in through `maxDisplayZoom`.
- `plans/app/investigation/map-zoom-clamping.md`
  - Recent zoom-clamping work intentionally chose `max_display_zoom` as the
    close-in camera limit to allow intended upsampling above `max_native_zoom`.

This creates an important hypothesis: the camera may now allow zooming through
`max_display_zoom`, while `MKTileOverlay.maximumZ` still stops overlay rendering
or tile requests at `max_native_zoom`. If MapKit does not automatically overzoom
tiles beyond `maximumZ` in this setup, the overlay will disappear at or above the
native/display boundary.

### Backend generation / publication path

- `backend/processing-svc/src/config.rs`
  - Defaults: `TILE_MIN_ZOOM=3`, `TILE_MAX_NATIVE_ZOOM=10`,
    `TILE_MAX_DISPLAY_ZOOM=12`, `TILE_BOUNDS=-125,24,-66,50`.
- `backend/processing-svc/src/process/message.rs`
  - A single processing queue message is processed for one VIIRS `tile_h/tile_v`
    granule.
  - `source_granules = vec![source_granule_for_message(processing_message)]`.
  - `generate_tile_set_for_granule_with_manifest(...)` builds a tile set for
    that one granule.
  - `publish_generated_tile_set(...)` publishes it and promotes it as latest.
- `backend/processing-svc/src/generate/orchestration.rs`
  - Generation plans tiles only through `config.tile_max_native_zoom`.
  - Fully transparent rendered tiles are filtered out.
  - Manifest `bounds` are derived from the non-empty native tile coverage.
- `backend/db-migrate/migrations/004_create_tile_sets.sql`
  - `tile_sets.latest` is a single moving pointer (`idx_tile_sets_latest_one`).
- `backend/db-migrate/migrations/003_create_processing_log.sql`
  - `processing_log` records one row per ingest item and stores `tile_set_id`,
    cloud/rejection counts, and terminal status.

This creates a second strong hypothesis: the local system may be publishing one
tile set per granule and promoting each as `latest`. If so, the latest manifest
will only show whichever single granule completed last, not a mosaic of all
processed queue items. That can explain a small East Coast-only overlay even
after the queue is empty.

## Investigation Questions

### A. Missing upsampling / no tiles at zoom >= 10

1. What does the latest manifest advertise?
   ```bash
   curl -s http://127.0.0.1:8080/api/v1/tiles/manifest \
     | jq '.data | {tile_set_id,bounds,tile_count,min_zoom,max_native_zoom,max_display_zoom,tile_url_template,source_granules}'
   ```
2. At the zoom level where the overlay disappears, what tile paths does the app
   request?
   - Add/enable temporary logging in `DarkSkyTileOverlay.loadTile(at:result:)`,
     or inspect existing network logs.
   - Record whether requests are for `z=10`, `z=11`, `z=12`, or stop entirely.
3. For a known visible area, do z10 tile URLs exist and return valid PNGs?
   ```bash
   curl -sI "<tile_url_template with z10/x/y substituted>"
   curl -s "<same-url>" -o /tmp/lh-z10.png && file /tmp/lh-z10.png
   ```
4. If the app requests z11/z12, are those expected to exist?
   - Current backend generation only plans through `TILE_MAX_NATIVE_ZOOM`.
   - If z11/z12 blobs are not generated, direct substitution of `{z}/{x}/{y}`
     will 404 unless another overzoom strategy exists.
5. If the app does not request anything above z10, does
   `MKTileOverlay.maximumZ = maxNativeZoom` prevent MapKit from displaying
   upsampled native tiles while the camera is allowed to zoom to
   `maxDisplayZoom`?

### B. Missing broad US coverage after processing the queue

1. Does latest point to one granule or a multi-granule tile set?
   ```bash
   curl -s http://127.0.0.1:8080/api/v1/tiles/manifest \
     | jq '.data | {tile_set_id,tile_count,bounds,source_granules}'
   ```
   If `source_granules | length` is `1`, the latest manifest is not a US mosaic.
2. How many tile sets were produced, and what are their bounds/counts?
   ```sql
   SELECT id, dataset_date, latest, tile_count, bounds, created_at
   FROM tile_sets
   ORDER BY created_at DESC
   LIMIT 50;
   ```
3. Are many queue items rejected or failed rather than published?
   ```sql
   SELECT status, count(*)
   FROM processing_log
   GROUP BY status
   ORDER BY status;

   SELECT product, tile_h, tile_v, status, tile_set_id,
          cloud_fraction, valid_pixel_count, rejected_pixel_count,
          error_message
   FROM processing_log
   ORDER BY updated_at DESC;
   ```
4. Did ingest discover/enqueue enough VIIRS `h/v` tiles for the configured US
   bounds?
   ```sql
   SELECT status, count(*)
   FROM ingest_log
   GROUP BY status
   ORDER BY status;

   SELECT product, tile_h, tile_v, status, count(*)
   FROM ingest_log
   GROUP BY product, tile_h, tile_v, status
   ORDER BY product, tile_h, tile_v, status;
   ```
5. Are messages hidden, dead-lettered, or left in a retry queue despite the main
   queue appearing empty?
   ```bash
   az storage message peek --queue-name viirs-processing --num-messages 32 \
     --connection-string "<azurite-connection-string>"

   az storage message peek --queue-name viirs-processing-deadletter --num-messages 32 \
     --connection-string "<azurite-connection-string>"
   ```
6. Do blob artifacts exist for every processed tile set, or only the latest
   one?
   ```bash
   az storage blob list \
     --connection-string "<azurite-connection-string>" \
     --container-name processed-tiles \
     --prefix "tiles/<tile_set_id>/" \
     --query "[].name" -o tsv | sort | head -50
   ```

## Root Cause Candidates

### A. Upsampling candidates

1. **App overlay maximum zoom mismatch.** The camera allows `max_display_zoom`,
   but `DarkSkyTileOverlay.maximumZ` is set to `maxNativeZoom`, so MapKit may stop
   drawing/requesting overlay tiles above native zoom.
2. **Backend does not generate display zoom tiles.** The backend only generates
   through `tile_max_native_zoom`; if the app requests z11/z12 URLs directly,
   those blobs will be missing.
3. **Contract ambiguity.** `max_display_zoom` may currently mean "safe camera
   presentation range" but not "there are addressable `{z}/{x}/{y}` blobs at
   that zoom." The app/backend contract needs to say which component performs
   overzooming.

### B. Coverage candidates

1. **Latest manifest is per-granule, not a mosaic.** Each processed queue message
   generates a one-granule tile set and promotes it as latest. Draining the queue
   then leaves latest pointing at only the last processed granule.
2. **Ingest/enqueue discovered only a subset of VIIRS tiles.** `ingest_log` may
   show only East Coast `tile_h/tile_v` entries, or many records may not have
   reached `enqueued`.
3. **Many granules were rejected or failed.** Cloud threshold, quality masks,
   HDF shape errors, `NoRenderableTiles`, or configured-bounds rejection may
   remove most expected coverage.
4. **Tiles exist but are filtered as empty.** `renderable_pixel_count == 0` tiles
   are intentionally not published; an overly strict classification/quality rule
   could shrink coverage.
5. **Blob/publication mismatch.** Tile-set DB rows may exist, but tile blobs may
   be missing, inaccessible, or under a different prefix/container than the
   manifest URL template.

## Candidate Fix Directions

Do not choose a fix until the evidence above identifies the failure mode.

### If the upsampling issue is app-side

- Decide whether product wants actual visible overzoom from `max_native_zoom` to
  `max_display_zoom`.
- If yes, implement an explicit overzoom strategy instead of relying on ambiguous
  `MKTileOverlay.maximumZ` behavior. Options include:
  - backend-generated display zoom tiles through `max_display_zoom`; or
  - a carefully tested client overzoom path that fetches native parent tiles and
    draws the correct child crop without recoloring or changing pixel classes.
- If no, set `max_display_zoom == max_native_zoom` in generated manifests or
  clamp the app camera at `maxNativeZoom` so the manifest does not promise a
  display range the app cannot render.

### If the coverage issue is per-granule latest publication

- Build an explicit dataset-date/product mosaic tile set from all accepted
  granules before publishing/promoting latest.
- Latest should point to a complete tile set whose `source_granules` contains all
  included ingests and whose `bounds`/`tile_count` describe the combined
  published coverage.
- Avoid promoting each individual granule as latest unless the product explicitly
  supports browsing per-granule tile sets.

### If ingest/enqueue is incomplete

- Fix the discovery/recovery path so every VIIRS tile intersecting configured
  US bounds is downloaded, validated, and enqueued.
- Add a coverage report that compares expected `tile_h/tile_v` coverage against
  `ingest_log` and `processing_log`.

### If quality/rejection is too aggressive

- Inspect rejection metadata and sample windows before loosening thresholds.
- Add tests around the rejected path so cloudy/bad pixels stay rejected while
  valid dark-sky evidence is not accidentally filtered out.

## Acceptance Criteria

1. The next agent can explain why no overlay appears at zoom `>= 10` with
   concrete evidence from app tile-request logs and manifest/blob checks.
2. If `max_display_zoom > max_native_zoom` remains in the manifest, the app has a
   verified rendering strategy for that range, or the backend publishes the
   required display zoom tiles.
3. The next agent can explain why only part of the East Coast is visible after
   draining the queue, backed by `tile_sets`, `processing_log`, `ingest_log`,
   queue/dead-letter, and blob-storage evidence.
4. The latest manifest either represents a deliberate complete tile set/mosaic or
   the product clearly exposes that it is only a single-granule tile set.
5. Missing-data fixes include regression tests or repeatable verification
   scripts that prevent silently publishing a tiny latest coverage area when a
   broader US tile set was expected.

## Out Of Scope For The Investigation Brief

- Implementing the fix before root cause is confirmed.
- Reworking the app legend/dataset metadata UI.
- Changing tile colors, classification labels, or radiance interpretation.
- Treating Apple `com.apple.GEO` base-map cache logs as custom overlay failures.

## References

- `app/LumenHorizon/LumenHorizon/Map/DarkSkyTileOverlay.swift` -
  `minimumZ`, `maximumZ`, `url(forTilePath:)`, `loadTile(at:result:)`.
- `app/LumenHorizon/AppCore/Sources/AppCore/Tiles/TileOverlayConfiguration.swift`
  - `minZoom`, `maxNativeZoom`, `maxDisplayZoom`, `cameraZoomRange`.
- `plans/app/investigation/map-zoom-clamping.md` - prior decision to allow
  camera zoom through `max_display_zoom`.
- `backend/processing-svc/src/config.rs` - tile zoom/bounds defaults.
- `backend/processing-svc/src/process/message.rs` - per-message generation and
  latest publication flow.
- `backend/processing-svc/src/generate/orchestration.rs` - native-zoom tile
  planning, non-empty tile filtering, manifest coverage derivation.
- `backend/processing-svc/src/publish.rs` - tile upload and latest pointer
  publication.
- `backend/db-migrate/migrations/003_create_processing_log.sql` -
  per-ingest processing outcome metadata.
- `backend/db-migrate/migrations/004_create_tile_sets.sql` - single latest tile
  set pointer.
