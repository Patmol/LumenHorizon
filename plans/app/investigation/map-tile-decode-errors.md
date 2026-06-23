# Investigation: `Failed to decode key` Console Errors During Map Rendering

## Status

Resolved for the app overlay path. Implemented the recommended app-side tile
loader under `APP-011`; Apple `com.apple.GEO` base-map cache messages may still
appear independently and should be filtered separately when diagnosing logs.

## Resolution

- `DarkSkyTileOverlay` now overrides `loadTile(at:result:)`, fetches tile data
  through URL loading, passes valid `200 image/png` bytes through unchanged, and
  returns transparent no-data for missing, non-200, empty, non-PNG,
  unsupported-URL, or transport-failed responses.
- The previous `about:blank` fallback is no longer used for effective tile
  loading; unsupported tile URLs complete as transparent no-data without a
  network request.
- Verification evidence:
  - App-target overlay-loader tests cover valid PNG pass-through, `404` XML,
    `200 text/html`, empty PNG responses, transport errors, and unsupported URL
    schemes.
  - `swift test --quiet` in `app/LumenHorizon/AppCore` passed.
  - `xcodebuild test ... -only-testing:LumenHorizonTests` on macOS passed.
  - iOS Simulator, macOS, and visionOS Simulator build checks passed.

## Owner Handoff

This document is a self-contained task brief. Read **Problem**, confirm the
**Root Cause** against the current code, then implement one of the **Candidate
Fixes** and meet the **Acceptance Criteria**. This issue is closely related to
`map-overlay-flicker.md` (the same failed tile loads likely cause both), and the
recommended fix here also addresses that flicker.

## Severity

Medium. The logs are noisy and obscure real diagnostics; if they correspond to
real failed overlay tile loads, they also degrade the visible overlay.

## Problem

The Xcode console is flooded with messages like:

```text
[0x9ea00c000][Dev] Failed to decode key: 48.94.8.1.256.0.0 t:35 kt:0 type: 35, rid: 4398
```

## Evidence And First Step: Disambiguate The Source

There are two very different log sources to separate before fixing anything:

1. MapKit / GEO base-map cache logs. The format
   `Failed to decode key: a.b.c.d... t:NN kt:NN type:NN rid:NNNN` is emitted by
   Apple's map stack (subsystem `com.apple.GEO`) for its internal vector/base
   tile cache. These are frequently benign and appear in normal MapKit usage,
   especially with flaky connectivity. They are **not** emitted by the app's
   custom `MKTileOverlay`.
2. The app's custom overlay errors. The dark-sky overlay surfaces failures as
   `<DarkSkyTileOverlay>: Error loading URL https://.../{z}/{x}/{y}.png: ...`
   (see the evidence captured in
   `plans/backend/investigation/local-tile-serving.md`).

Action: in Console.app (or Xcode) filter by subsystem `com.apple.GEO` and by the
overlay class name to confirm which messages are present. If only the GEO
messages appear and overlay tiles render, the `Failed to decode key` lines are
base-map noise and the fix is "confirm benign / filter logs," not an overlay
change.

## Root Cause (when the overlay is implicated)

When a requested tile blob does not exist, the tile host returns a non-image
error body rather than an image:

- Local Azurite returns HTTP `404` with an XML `BlobNotFound` body.
- A misconfigured/again-unreachable host returns HTML/JSON or a DNS error.

The custom overlay does not currently filter these responses:

- `app/LumenHorizon/LumenHorizon/Map/DarkSkyTileOverlay.swift`
  - It overrides `url(forTilePath:)` only. It does **not** override
    `loadTile(at:result:)`, so MapKit's default loader hands whatever bytes come
    back to the renderer, which then fails to decode a non-PNG body.
  - `url(forTilePath:)` falls back to `URL(string: "about:blank")!` if template
    substitution ever fails, which would also produce an undecodable response.

## Reproduction

1. Point the app at a manifest whose tile set has partial coverage (some
   `{z}/{x}/{y}` tiles missing) or a stale host (see
   `plans/backend/investigation/local-tile-serving.md`).
2. Pan/zoom over an area where tiles are missing.
3. Observe console errors and missing/flashing overlay tiles.

## Candidate Fixes

Pick one; record the decision and rationale.

### Option A - Gracefully handle non-image responses in the overlay (recommended)

Override `loadTile(at:result:)` in `DarkSkyTileOverlay` to treat missing or
non-image responses as transparent no-data instead of decode failures.

1. Fetch the tile with a shared `URLSession` (respecting cache headers).
2. If the response is not HTTP `200`, or the `Content-Type` is not
   `image/png` (or the body is empty), call `result(Data(), nil)` (empty data =
   transparent, no error) instead of passing the bad body through.
3. On success, pass the PNG bytes through unchanged. Do not recolor or
   reinterpret pixels (per engineering standards).
4. Keep the URL-building path; remove the `about:blank` fallback in favor of an
   explicit empty-tile result if substitution fails.

- Tradeoff: adds a custom loader to maintain, but removes decode-error noise,
  prevents flicker (`map-overlay-flicker.md`), and matches the product rule that
  transparent/no-data pixels mean "no coverage."

### Option B - Fix the source so missing tiles return empty/transparent

Have the tile host return `204 No Content` or a transparent 1x1 PNG for missing
tiles.

- Tradeoff: this requires a server-side tile layer. The current local design
  loads tiles directly from blob storage / CDN, and a dev gateway tile server is
  explicitly deferred (see `plans/backend/investigation/local-tile-serving.md`).
  Not preferred for resolving the app-side noise.

## Acceptance Criteria

1. The `Failed to decode key` lines are confirmed as either benign GEO base-map
   logs (documented as such) or eliminated for the overlay path.
2. With a partial-coverage manifest, panning over missing tiles produces no
   overlay decode errors and no console spam attributable to the overlay.
3. Missing tiles render as transparent (no broken-tile artifacts).
4. Valid PNG tiles still render unchanged (no recoloring).

## Out Of Scope

- Backend tile host configuration (`plans/backend/investigation/local-tile-serving.md`).
- Zoom clamping (`map-zoom-clamping.md`).
- Coverage/alignment correctness (`map-overlay-misalignment.md`).

## References

- `app/LumenHorizon/LumenHorizon/Map/DarkSkyTileOverlay.swift` -
  `url(forTilePath:)`, missing `loadTile(at:result:)` override.
- `app/LumenHorizon/AppCore/Sources/AppCore/Tiles/TileURLTemplate.swift` -
  `url(z:x:y:)`.
- `plans/backend/investigation/local-tile-serving.md` - related host issue and
  the `<DarkSkyTileOverlay>: Error loading URL` evidence.
- `plans/app/20-engineering-standards.md` - "Do not recolor or reinterpret PNG
  tile pixels in the client."
- `plans/app/status/gap-register.md` - closed `APP-011`.
