# App Gap Register

This register tracks active native app product gaps.

## Severity

| Severity | Meaning |
| --- | --- |
| Critical | Blocks core app correctness or can misrepresent backend data. |
| High | Blocks reliable app validation or core map functionality. |
| Medium | Important hardening, usability, or evidence gap. |
| Low | Documentation or polish gap. |

## Active Gaps

| ID | Severity | Area | Gap | Target chunk |
| --- | --- | --- | --- | --- |
| APP-004 | High | Product copy | No app UX exists to explain VIIRS radiance evidence, transparent pixels, limitations, or non-goals. | Chunk 4 |
| APP-005 | Medium | Tile-set selection | No historical/latest tile-set browser or selected tile-set persistence exists. | Chunk 5 |
| APP-006 | Medium | Offline resilience | No metadata cache, freshness labels, or offline fallback exists. | Chunk 6 |
| APP-007 | Medium | Platform polish | iOS, iPadOS, macOS, and visionOS navigation, commands, layout, and accessibility are undefined. | Chunk 7 |
| APP-008 | High | Validation | No app build/test commands, fixtures, UI smoke tests, or CI jobs exist. | Chunk 8 |
| APP-009 | Low | Launch readiness | No app developer guide, privacy note, or local backend smoke guide exists. | Chunk 9 |

## Closed Gaps

| ID | Area | Resolution |
| --- | --- | --- |
| APP-001 | Project foundation | Multiplatform Xcode project (iOS/macOS/visionOS) builds in Debug and Release; the shared `AppCore` Swift package provides app configuration, environment-driven API base URLs, and map defaults; environment values flow from Debug/Release build settings through `Info.plist` into `AppRuntimeConfiguration`; `AppCore` unit tests cover configuration parsing and the release-points-at-local guard. |
| APP-002 | Backend contracts | `BackendClient` provides typed requests for latest manifest, manifest by id, tile-set pages (`next_cursor`), and tile classes over strict DTOs (`ApiEnvelope`, `TileManifest`, `TileSetSummary`, `TileClasses`) with typed `BackendError`/`BackendErrorCode` mapping and `TileURLTemplate` placeholder validation; `AppCore` tests cover decoding, error envelopes, pagination, and malformed/placeholder-missing manifests. |
| APP-003 | Map rendering | `TileOverlayConfiguration`, `MapOverlayState`, and `MapViewModel` plus the app-target `DarkSkyTileOverlay` and cross-platform `DarkSkyMapView` render the backend PNG overlay with zoom/bounds guards, tile-set-change overlay replacement, live opacity, and loading/empty/unavailable/retry states; `AppCore` tests cover configuration validation, URL construction, and state transitions. Live local tile imagery now renders after the backend local-dev tile-serving fix (anonymous Azurite `processed-tiles` reads plus a local `TILE_CDN_BASE_URL`; see [../../backend/investigation/local-tile-serving.md](../../backend/investigation/local-tile-serving.md)). |
| APP-010 | Map zoom clamping | `TileOverlayConfiguration` derives camera center-coordinate distance limits from manifest `min_zoom` and `max_display_zoom`, and `DarkSkyMapView` applies `MKMapView.CameraZoomRange` plus a coverage `CameraBoundary` whenever an overlay tile set is active. |
| APP-011 | Map tile loading | `DarkSkyTileOverlay` now overrides `loadTile(at:result:)` so valid `200 image/png` tile bytes pass through unchanged while missing, non-200, empty, non-PNG, unsupported-URL, and transport-failed tile responses complete as transparent no-data instead of surfacing overlay decode failures. |
| APP-012 | Map overlay stability | `MapViewModel` keeps a last-good render configuration during manifest reload/retry loading, `ContentView` renders from that stable configuration, and missing tile responses are transparent no-data, preventing overlay blink during same-tile-set refreshes and partial-coverage tile flicker. |
| APP-013 | Map overlay alignment | `processing-svc` now renders partial edge tiles into their correct pixel subwindow with transparent fill outside source coverage, tracks non-empty rendered evidence, publishes only evidence-bearing tiles, and derives manifest `bounds`/`tile_count` from the published non-empty tile coverage. |
