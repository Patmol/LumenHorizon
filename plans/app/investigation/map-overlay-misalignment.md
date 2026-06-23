# Investigation: Overlay Is Stretched / Does Not Cover The Correct Zone

## Status

Resolved. Implemented backend rendering and manifest-coverage fixes under
`APP-013`.

## Resolution

The confirmed root cause was a backend edge-tile rendering bug plus loose
coverage metadata:

- `processing-svc` clipped partial edge tiles to the source granule bounds, then
  resized that clipped raster window to a full PNG tile. For edge tiles this
  stretched the available source samples across the full `{z}/{x}/{y}` tile,
  making the overlay content appear scaled or shifted even though app-side
  MapKit geometry and XYZ math were correct.
- Manifests also used planned generation coverage rather than the published
  non-empty evidence tile coverage, so `bounds` and `tile_count` could describe
  more coverage than the visible evidence-bearing tiles.

Implemented fixes:

- `backend/processing-svc/src/generate/window.rs` now returns both the clipped
  source raster window and the target pixel subwindow inside the full tile.
- `backend/processing-svc/src/generate/rendering.rs` expands clipped samples
  into a full tile-sized sample grid with fill values outside the source
  subwindow, so outside-source pixels render transparent instead of stretching.
- `backend/processing-svc/src/publish.rs::RenderedTile` records
  `renderable_pixel_count`; generation filters fully transparent tiles before
  manifest construction/publication.
- `backend/shared/src/slippy_tiles.rs` exposes a tile-coordinate bounds union
  helper, and generation derives manifest `bounds` from non-empty native tile
  coverage.
- Zero-evidence tile sets now fail with a typed `NoRenderableTiles` generation
  error instead of publishing/promoting an empty manifest.

Verification evidence:

- `cd backend && cargo fmt --all -- --check` passed.
- `cd backend && cargo test --workspace --quiet` passed.
- `cd backend && cargo clippy --workspace --all-targets -- -D warnings` passed.
- `cd app/LumenHorizon/AppCore && swift test --quiet` passed.

## Owner Handoff

This document is a self-contained task brief. Read **Problem**, confirm the
**Root Cause** candidates against the current code and a running backend, then
implement the appropriate **Candidate Fix** and meet the **Acceptance
Criteria**. The most likely cause is backend coverage/manifest data, not app
geometry, so verification against a running backend is required.

## Severity

High. A misplaced or stretched dark-sky layer can misrepresent backend data,
which violates a core product rule.

## Problem

The overlay sometimes appears stretched and does not cover the geographic zone
it should. It looks scaled-up or shifted relative to the underlying base map.

## What Is NOT The Cause (verified in code)

- Overlay bounding geometry is correct.
  `app/LumenHorizon/LumenHorizon/Map/DarkSkyTileOverlay.swift`,
  `boundingMapRect` builds the rect from `north/west` (top-left) and
  `south/east` (bottom-right) using `min(...)` origin and `abs(...)` width/height.
  This is the correct MapKit convention (y increases southward).
- URL construction is a straight substitution.
  `app/LumenHorizon/AppCore/Sources/AppCore/Tiles/TileURLTemplate.swift`,
  `url(z:x:y:)` replaces `{z}/{x}/{y}` with the integer path values only.
- Tiling scheme matches MapKit. The backend uses standard XYZ / Web Mercator,
  `backend/shared/src/slippy_tiles.rs`:
  - `tile_y_to_latitude` uses `PI * (1 - 2*y/n)` then `sinh().atan()`.
  - This is the same convention `MKTileOverlay` expects (no TMS y-flip), so
    there is no projection or row-flip mismatch between backend tiles and the
    client.

## Root Cause Candidates (verify against a running backend)

1. Upsampling above native zoom (perceived blur/stretch). The manifest's
   `max_native_zoom` (e.g. 10) is below `max_display_zoom` (e.g. 12). Between
   native and display zoom, MapKit upsamples native tiles, which looks soft or
   "stretched" although it remains geographically aligned. This overlaps
   `map-zoom-clamping.md`.
2. Advertised bounds wider than actual tile coverage (most likely for the
   "wrong zone" symptom). The manifest `bounds` advertise the full configured
   region (e.g. continental US, `west -125 / south 24 / east -66 / north 50` in
   `backend/processing-svc/src/config.rs`), but a dev tile set generated from a
   single VIIRS granule only renders tiles where that granule had data. Tiles
   exist only in a sub-area; the rest is missing. The overlay then appears only
   in a small zone and can look "stretched" where sparse low-zoom tiles cover a
   large area.
3. Tile pixel size vs manifest `tile_size` mismatch (true stretch). If rendered
   PNGs are a different pixel size than the manifest's `tile_size` (256), MapKit
   scales the image into the tile slot, producing genuine stretch. Verify actual
   PNG dimensions.
4. Backend rendering/coordinate bug (lower likelihood). Tiles whose content does
   not match their `{z}/{x}/{y}` would place data in the wrong place. The slippy
   math is verified, so this is unlikely, but rule it out with a visual
   spot-check.

## Reproduction / Verification

1. Inspect advertised bounds and zoom:
   ```bash
   curl -s http://127.0.0.1:8080/api/v1/tiles/manifest \
     | jq '.data | {bounds, min_zoom, max_native_zoom, max_display_zoom, tile_size, tile_count}'
   ```
2. List actual tiles in Azurite and compare coverage to the advertised bounds:
   ```bash
   az storage blob list \
     --connection-string "<azurite-connection-string>" \
     --container-name processed-tiles \
     --prefix "tiles/<tile_set_id>/" \
     --query "[].name" -o tsv | sort | head
   ```
   Confirm the `{z}/{x}/{y}` range maps to the advertised bounds, not a tiny
   sub-area.
3. Pixel-size check on a known tile:
   ```bash
   curl -s "<substituted tile URL>" -o tile.png && file tile.png
   ```
   Confirm dimensions equal manifest `tile_size`.
4. Geographic spot-check: compute `{z}/{x}/{y}` for a known city at native zoom
   and confirm the light-pollution pattern lands on that city.

## Candidate Fixes

Pick based on what verification shows; record the decision and rationale.

- If coverage mismatch (candidate 2): backend/processing should set manifest
  `bounds` to the actual rendered extent (or generate the full advertised
  coverage). Primarily a `processing-svc` change
  (`backend/processing-svc/src/manifest.rs`). App-side, optionally set
  `mapView.cameraBoundary` from `boundingMapRect` and surface an "outside
  coverage" affordance per `plans/app/50-map-and-data-experience.md`.
- If upsampling perception (candidate 1): clamp display to `max_native_zoom`
  (overlaps `map-zoom-clamping.md`) or accept upsampling and document it.
- If tile-size mismatch (candidate 3): fix the manifest `tile_size` or the
  renderer's tile pixel size so they agree.
- If rendering/coordinate bug (candidate 4): fix in `processing-svc` tile
  rendering; add a regression test for a known coordinate.

## Acceptance Criteria

1. At native zoom, a known location's overlay pattern aligns with its real
   geography (spot-check passes).
2. Advertised manifest `bounds` match the actual rendered tile coverage (no
   large empty advertised area).
3. Tiles are not scaled due to a `tile_size` mismatch.
4. Any residual softness above `max_native_zoom` is understood and is upsampling,
   not misalignment.

## Out Of Scope

- Zoom-range clamping implementation (`map-zoom-clamping.md`).
- Tile decode-error handling and flicker (`map-tile-decode-errors.md`,
  `map-overlay-flicker.md`).
- Backend tile host configuration (`plans/backend/investigation/local-tile-serving.md`).

## References

- `app/LumenHorizon/LumenHorizon/Map/DarkSkyTileOverlay.swift` -
  `boundingMapRect`, `tileSize`.
- `app/LumenHorizon/AppCore/Sources/AppCore/Tiles/TileURLTemplate.swift` -
  `url(z:x:y:)`.
- `backend/shared/src/slippy_tiles.rs` - `tile_y_to_latitude`,
  `latitude_to_tile_y`, `tile_bounds` (XYZ / Web Mercator).
- `backend/processing-svc/src/manifest.rs` - manifest `bounds`/zoom building.
- `backend/processing-svc/src/config.rs` - `tile_bounds`, zoom defaults,
  `tile_size`.
- `plans/app/50-map-and-data-experience.md` - bounds/coverage UX.
- `plans/app/status/gap-register.md` - closed `APP-013`.
