# Investigation: Map Zoom Is Not Clamped To The Overlay's Supported Range

## Status

Resolved under `APP-010`.

## Resolution

- `TileOverlayConfiguration` now exposes a MapKit-free
  `cameraZoomRange` helper that derives center-coordinate distance limits from
  manifest metadata.
- The far-out limit is derived from `min_zoom`; the close-in limit is derived
  from `max_display_zoom` so the app honors the manifest's useful display range
  and allows intended upsampling above `max_native_zoom`.
- `DarkSkyMapView` applies `MKMapView.CameraZoomRange` and an
  `MKMapView.CameraBoundary` from the active overlay whenever the tile set
  changes, and clears both constraints when no overlay is active.
- Verification evidence:
  - AppCore unit tests cover manifest-derived zoom distances, ordering,
    `max_display_zoom` close-limit behavior, and latitude effects without
    MapKit rendering.
  - App-target `LumenHorizonTests` build and pass with the MapKit host wiring.

## Owner Handoff

This document is a self-contained task brief. You can act on it without prior
context. Read **Problem**, confirm the **Root Cause** against the current code,
then implement one of the **Candidate Fixes** and meet the **Acceptance
Criteria**. This issue is independent of the tile-loading issues
(`map-tile-decode-errors.md`, `map-overlay-flicker.md`,
`map-overlay-misalignment.md`) and can be fixed on its own.

## Severity

Medium. Core map browsing still works, but the overlay silently disappears
whenever the user zooms outside the supported band, which reads as a broken or
missing data layer.

## Problem

When the map zoom is greater than ~10 or less than ~4, no dark-sky overlay is
shown. The camera is free to move anywhere, so the user can easily leave the
zoom band where backend tiles exist and is left with a base map and no overlay
and no explanation. The desired behavior is for the map to constrain the zoom
to the overlay's supported range so the overlay is always present while it is
the active data layer.

## Evidence

- The demo/preview manifest advertises `min_zoom: 3`, `max_native_zoom: 10`,
  `max_display_zoom: 12` (see the preview manifest in
  `app/LumenHorizon/LumenHorizon/ContentView.swift` and the backend defaults in
  `backend/processing-svc/src/config.rs`).
- Observed behavior: overlay visible roughly between zoom 4 and 10, blank
  outside that band.

## Root Cause

The `MKMapView` host never constrains the camera. There is no
`cameraZoomRange` / `setCameraZoomRange(...)` and no `cameraBoundary` anywhere
in the map host.

- `app/LumenHorizon/LumenHorizon/Map/DarkSkyMapView.swift`
  - `makeMapView(context:)` creates a plain `MKMapView`, sets the delegate, and
    returns it. No zoom range or camera boundary is configured.
  - `updateMapView(_:context:)` updates opacity and replaces the overlay on
    tile-set change, but never constrains the camera.
  - `Coordinator.regionDidChangeAnimated` only *reports* the derived tile zoom
    via `onZoomChange`; it does not clamp anything.
- `app/LumenHorizon/LumenHorizon/Map/DarkSkyTileOverlay.swift`
  - `init(configuration:)` sets `minimumZ = configuration.minZoom` and
    `maximumZ = configuration.maxNativeZoom`. These only gate which tiles
    MapKit *requests* (and how it upsamples), not where the camera can go.

Net effect: below `minZoom`, MapKit requests no tiles; above `maxNativeZoom`,
MapKit upsamples native tiles up to `maxDisplayZoom` and then stops. Because the
camera is unconstrained, the user trivially navigates outside the band and the
overlay becomes blank.

## Reproduction

1. Run the app against any backend that returns a valid manifest.
2. Pinch-zoom in past native zoom (`max_native_zoom`) or zoom far out below
   `min_zoom`.
3. Observe the overlay disappears with no explanation while the base map
   remains.

## Candidate Fixes

Pick one; record the decision and rationale.

### Option A - Constrain the camera from the manifest (recommended)

Derive an `MKMapView.CameraZoomRange` from the active
`TileOverlayConfiguration` and apply it whenever the configuration changes.

1. Convert tile zoom to a center-coordinate distance. Web Mercator
   ground resolution is `metersPerPixel = 156543.03392 * cos(latitude) / 2^z`.
   Map the manifest's farthest zoom (`minZoom`) to
   `maxCenterCoordinateDistance` and the closest useful zoom
   (`maxNativeZoom`, or `maxDisplayZoom` if upsampling is acceptable) to
   `minCenterCoordinateDistance`.
2. In `DarkSkyMapView.makeMapView` / `updateMapView`, call
   `mapView.setCameraZoomRange(_:animated:)` with the derived range when the
   tile set changes.
3. Optionally also set
   `mapView.cameraBoundary = MKMapView.CameraBoundary(mapRect: boundingMapRect)`
   using the overlay's coverage so the user cannot pan into empty regions that
   imply coverage.
4. Keep the zoom-to-distance conversion in a small, pure, unit-testable helper
   (ideally in `AppCore` next to `TileOverlayConfiguration`) so it can be tested
   without MapKit rendering.

- Tradeoff: requires a correct zoom-to-distance conversion, but matches the
  manifest contract and avoids hardcoding 4/10. Recommended because the band is
  driven by data, not magic numbers.

### Option B - Clamp reactively in the delegate

In `regionDidChangeAnimated`, detect when the derived zoom leaves the range and
call `setRegion`/`setCamera` to push it back.

- Tradeoff: simpler to reason about per-frame but produces visible jitter and
  fights user gestures. Not recommended.

### Decision To Record

Whether to clamp the close end at `maxNativeZoom` (no upsampling, crisper) or
`maxDisplayZoom` (allows MapKit's upsampling, matches the manifest's stated
display range). Recommend `maxDisplayZoom` to honor the contract, and revisit if
upsampling looks too soft.

## Acceptance Criteria

1. The camera cannot zoom closer than the overlay's closest supported zoom or
   farther than its farthest supported zoom; the overlay is present throughout
   the allowed band.
2. The zoom limits are derived from the manifest
   (`min_zoom`/`max_native_zoom`/`max_display_zoom`), not hardcoded.
3. The zoom-to-distance conversion has unit tests that do not require MapKit
   rendering.
4. Switching tile sets updates the limits to the new manifest's range.

## Out Of Scope

- Tile-loading robustness (`map-tile-decode-errors.md`).
- Overlay flicker (`map-overlay-flicker.md`).
- Coverage/alignment correctness (`map-overlay-misalignment.md`).
- Backend manifest contents.

## References

- `app/LumenHorizon/LumenHorizon/Map/DarkSkyMapView.swift` - map host,
  `makeMapView`, `updateMapView`, `Coordinator.regionDidChangeAnimated`,
  `tileZoom(of:)`.
- `app/LumenHorizon/LumenHorizon/Map/DarkSkyTileOverlay.swift` - `minimumZ` /
  `maximumZ` / `boundingMapRect`.
- `app/LumenHorizon/AppCore/Sources/AppCore/Tiles/TileOverlayConfiguration.swift`
  - `minZoom` / `maxNativeZoom` / `maxDisplayZoom` / `bounds`.
- `plans/app/50-map-and-data-experience.md` - "Map Bounds And Zoom".
- `plans/app/status/gap-register.md` - register `APP-010` if adopted.
