# App Status Overview

## Current Snapshot

The native app now renders the backend dark-sky tile set over MapKit. A multiplatform Xcode project targets iOS, macOS, and visionOS from shared code; the `AppCore` Swift package provides environment-driven configuration, a typed backend client with strict contract models, and the pure overlay/state logic that drives the map. The app fetches the latest manifest, builds a validated `MKTileOverlay`, and exposes loading, empty, unavailable/retry, and opacity controls.

## Already Done

- App planning structure exists under `plans/app`.
- Backend client contracts are identified from existing backend docs and code.
- Initial chunk roadmap, architecture, standards, integration, testing, and gap docs are drafted.
- Chunk 1 (Xcode project and shared app foundation) is complete:
  - Multiplatform Xcode project with iOS, macOS, and visionOS support, building in Debug and Release.
  - Shared `AppCore` Swift package with `AppConfiguration`, `APIConfiguration`, `AppEnvironment`, and `MapDefaults`.
  - Environment-driven API base URLs for local, preview, and release, wired through build settings and `Info.plist` into `AppRuntimeConfiguration`, with a guard preventing release builds from pointing at a local backend.
  - Basic SwiftUI app shell with navigation, a MapKit map, and platform-specific window/scene setup.
  - `AppCore` unit tests covering configuration parsing and validation.
- Chunk 2 (backend client and contract models) is complete:
  - `BackendClient` with typed requests for latest manifest, manifest by id, tile sets (with `next_cursor` paging), and tile classes.
  - Strict DTOs for `ApiEnvelope`, `TileManifest`, `TileSetSummary`, `TileClasses`, and typed `BackendError`/`BackendErrorCode`.
  - `TileURLTemplate` placeholder validation and cache-metadata models.
  - `AppCore` tests covering decoding, backend error mapping, pagination, and malformed manifests.
- Chunk 3 (MapKit tile overlay MVP) is complete:
  - `TileOverlayConfiguration` validates a manifest into MapKit-safe overlay parameters; `MapOverlayState` encodes idle/loading/ready/empty/unavailable transitions; `MapViewModel` drives them from `BackendClient` and owns overlay opacity.
  - App-target `DarkSkyTileOverlay` (`MKTileOverlay`) plus a cross-platform `DarkSkyMapView` host that replaces stale overlays on tile-set change, applies live opacity, constrains camera zoom/bounds from manifest metadata while the overlay is active, and treats missing/non-image tile responses as transparent no-data instead of decode failures.
  - `ContentView` shows loading, empty, unavailable/retry states and an opacity control.
  - `AppCore` tests cover configuration validation, URL construction, manifest-derived camera zoom limits, state mapping, and view-model transitions, including last-good overlay retention during reloads; app-target tests cover tile-loader response handling.
  - Backend tile generation now preserves geographic alignment for partial edge tiles by rendering clipped source samples into the correct tile subwindow with transparent fill, and manifests describe published non-empty tile coverage.
  - macOS App Sandbox outgoing-connections entitlement and a local-networking ATS exception are required for the app to reach a local HTTP backend (see Known Limitations).

## Not Started

- Legend and dataset metadata UI.
- Tile-set selection and freshness UX.
- Offline cache and resilience.
- Native polish and platform adaptation.
- App UI tests and CI.
- App developer guide.

## Current Roadmap Position

The active app roadmap is at [../00-implementation-roadmap.md](../00-implementation-roadmap.md). Chunks 0-3 (architecture baseline, project/shared foundation, backend client, and the MapKit tile overlay MVP) are complete. Chunk 4 (legend, dataset metadata, and product copy) is the next chunk. Chunks 5-6 build tile-set selection and cache resilience. Chunks 7-9 cover native polish, quality gates, and local launch readiness.

## Known Limitations

- Local tile imagery does not render yet. Backend-generated manifests carry the production CDN host (`tiles.lumenhorizon.com`) in `tile_url_template`, which is unreachable from local simulators, and locally rendered tiles in Azurite are not exposed as anonymously-readable tile URLs. The app overlay/zoom/bounds logic is verified correct (it requests the right tiles, confirmed by tile-fetch logs), so the gap is backend local-dev tile serving. See [../../backend/investigation/local-tile-serving.md](../../backend/investigation/local-tile-serving.md).

## Remaining Work

See [gap-register.md](gap-register.md). Project foundation (APP-001), backend client (APP-002), map rendering (APP-003), overlay zoom clamping (APP-010), overlay tile decode handling (APP-011), overlay flicker stabilization (APP-012), and overlay alignment (APP-013) are closed; the remaining app implementation gaps are open from Chunk 4 onward.
