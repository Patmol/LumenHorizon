# Investigation: Overlay Tiles Appear Then Immediately Disappear

## Status

Resolved. Implemented app-side fixes under `APP-012`: graceful tile loading
for missing/non-image tile responses and last-good overlay retention during
manifest refreshes.

## Resolution

- `DarkSkyTileOverlay` now overrides `loadTile(at:result:)`, passes valid PNG
  bytes through unchanged, and treats missing, non-200, empty, non-PNG,
  unsupported-URL, and transport-failed tile requests as transparent no-data.
- `MapViewModel` exposes `renderableConfiguration`, retaining the latest ready
  configuration while a reload/retry is `.loading` and clearing it when the
  latest manifest resolves to empty or unavailable.
- `ContentView` renders from `renderableConfiguration`, so a same-tile-set
  reload no longer removes and re-adds the overlay while refresh is in flight.
- Verification evidence:
  - `swift test --quiet` in `app/LumenHorizon/AppCore` passed.
  - `xcodebuild test ... -only-testing:LumenHorizonTests` on macOS passed for
    overlay-loader unit tests.
  - iOS Simulator, macOS, and visionOS Simulator build checks passed.

## Owner Handoff

This document is a self-contained task brief. Read **Problem**, confirm the
**Root Cause** against the current code, then implement one of the **Candidate
Fixes** and meet the **Acceptance Criteria**. This issue is very likely the same
underlying cause as `map-tile-decode-errors.md`; fix that first and re-test.

## Severity

Medium-High. A flickering overlay looks broken and undermines trust in the data
layer even when the backend is healthy.

## Problem

Some overlay tiles render briefly and then disappear, sometimes repeatedly while
panning or zooming.

## What Is NOT The Cause (verified)

The overlay lifecycle is already guarded correctly, so continuous
remove/re-add churn is **not** the cause:

- `app/LumenHorizon/LumenHorizon/Map/DarkSkyMapView.swift`,
  `updateMapView(_:context:)`:
  - It computes `newTileSetId = configuration?.tileSetId` and
    `currentTileSetId = coordinator.overlay?.configuration.tileSetId`, then
    `guard newTileSetId != currentTileSetId else { return }`.
  - So when SwiftUI re-renders for unrelated reasons (zoom reporting, opacity
    changes), the overlay is **not** rebuilt.
- `Coordinator.regionDidChangeAnimated` dispatches `onZoomChange` asynchronously
  to the main actor, which updates `@State zoomLevel` in `ContentView` and
  triggers `updateUIView`. This is benign given the guard above, but it does
  mean `updateUIView` runs on every region change.

## Root Cause (most likely)

1. Failed tile loads (primary). Tiles that 404 / fail to decode (see
   `map-tile-decode-errors.md`) are dropped by MapKit. On each
   `regionDidChangeAnimated`, MapKit re-requests the visible tiles and drops the
   failing ones again, producing a visible appear/disappear cycle in areas with
   partial coverage or a stale tile host.
2. Transient `nil` configuration during refresh (secondary). On `load()` /
   `retry()`, `MapViewModel.state` transitions to `.loading`, during which
   `state.configuration` is `nil`. That legitimately drives
   `configuration == nil`, so the guard passes and the overlay is removed, then
   re-added when `.resolved` returns. If a refresh happens while viewing, the
   overlay blinks even for the same tile set.

- `app/LumenHorizon/AppCore/Sources/AppCore/Tiles/MapViewModel.swift` -
  `load()` sets `state = .loading` then `.resolved(from: manifest)`.
- `app/LumenHorizon/AppCore/Sources/AppCore/Tiles/MapOverlayState.swift` -
  `configuration` is `nil` for non-resolved states.

## Reproduction

1. Use a manifest with partial coverage or a stale tile host (see
   `plans/backend/investigation/local-tile-serving.md`).
2. Pan/zoom over the covered region and watch tiles flash in and out.
3. Separately, trigger a reload/retry while the overlay is visible and watch it
   blink even when the same tile set resolves.

## Candidate Fixes

Pick one or combine; record the decision and rationale.

### Option A - Graceful tile loading (recommended, primary)

Implement the `loadTile(at:result:)` handling from `map-tile-decode-errors.md`
so failed/missing tiles become transparent no-data instead of being dropped.
This removes the re-request/re-drop flicker for partial coverage.

### Option B - Preserve the last-good overlay across refreshes (secondary)

Avoid tearing down the overlay when the configuration only transitions through
`.loading` for the **same** tile set:

1. When the new configuration is `nil` but a reload is in progress, keep the
   current overlay until a new `.resolved` configuration arrives.
2. Only remove the overlay when the tile set actually changes or the data is
   genuinely unavailable.

- Tradeoff: keeps the map stable during refresh, but the view host must
  distinguish "loading the same set" from "no data." Consider exposing the last
  resolved configuration from `MapViewModel`.

### Option C - Confirm it is not an opacity/alpha artifact

Verify `opacity` is non-zero (default `0.85`) and the renderer alpha is applied
after `rendererFor` returns. This is unlikely but cheap to rule out.

## Acceptance Criteria

1. With full coverage, the overlay is stable across pan/zoom (no flicker).
2. With partial coverage, present tiles stay rendered; missing tiles are
   transparent (no flashing).
3. A reload/retry that re-resolves the same tile set does not blank the overlay.

## Out Of Scope

- Zoom clamping (`map-zoom-clamping.md`).
- Coverage/alignment correctness (`map-overlay-misalignment.md`).
- Backend tile host configuration (`plans/backend/investigation/local-tile-serving.md`).

## References

- `app/LumenHorizon/LumenHorizon/Map/DarkSkyMapView.swift` - overlay lifecycle
  guard, `regionDidChangeAnimated`.
- `app/LumenHorizon/AppCore/Sources/AppCore/Tiles/MapViewModel.swift` -
  `load()` / `retry()` state transitions.
- `app/LumenHorizon/AppCore/Sources/AppCore/Tiles/MapOverlayState.swift` -
  `configuration` per state.
- `plans/app/investigation/map-tile-decode-errors.md` - shared root cause/fix.
- `plans/app/status/gap-register.md` - closed `APP-012`.
