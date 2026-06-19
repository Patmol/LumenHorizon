# 00. App Implementation Roadmap

This draft roadmap mirrors the backend chunk style while focusing on a native SwiftUI app for iOS, macOS, and visionOS. Chunks should remain independently reviewable and should not require backend contract changes unless the affected backend docs and tests are updated in the same chunk.

## Chunk Rules

- A chunk must be independently reviewable and have clear verification.
- The first app version consumes existing anonymous `/api/v1/tiles/*` routes only.
- Map copy must describe `radiance-dark-sky-v1` as VIIRS radiance-based dark-sky evidence, not measured Bortle class, SQM readings, or certified observing quality.
- Backend URL, cache behavior, and debug fixtures must be environment-driven and not hardcoded in UI code.
- API contract changes must update [reference/api-and-data-contracts.md](reference/api-and-data-contracts.md), backend docs, and tests together.

## Chunk 0 - Product And Architecture Baseline

Purpose: define the app product promise, supported platforms, module boundaries, and backend dependency model.

Deliverables:

- iOS, macOS, and visionOS product goals and non-goals.
- SwiftUI/MapKit app architecture.
- Backend API dependency map.
- Initial app reference docs and status tracking.

Verification:

- Plan links resolve from [overview.md](overview.md).
- Architecture docs match the intended app repository layout.

## Chunk 1 - Xcode Project And Shared App Foundation

Purpose: create a native Apple project foundation that can target iOS, macOS, and visionOS from shared code.

Deliverables:

- Xcode project or workspace with iOS, macOS, and visionOS targets.
- Shared Swift package or target for app core models, networking, tile configuration, and map state.
- Build configurations for local, preview, and release API base URLs.
- Basic app shell with SwiftUI navigation and platform-specific window/scene setup.

Verification:

- iOS, macOS, and visionOS targets build from a clean clone where the required SDKs and simulator runtimes are available.
- Unit tests compile for shared app code.
- No release configuration points at a local-only backend URL.

## Chunk 2 - Backend Client And Contract Models

Purpose: consume existing backend API envelopes and tile contracts safely.

Deliverables:

- `BackendClient` with typed requests for latest manifest, tile manifest by id, tile sets, and tile classes.
- Strict DTOs for `ApiEnvelope`, `TileManifest`, `TileSetSummary`, `TileClasses`, and backend errors.
- URL-template validation for `{z}`, `{x}`, and `{y}` placeholders.
- Cache metadata for latest manifest and class metadata.

Verification:

- Unit tests cover successful decoding, backend error envelopes, malformed manifests, missing URL placeholders, and unknown additive fields.
- Fixture JSON stays aligned with backend API examples.

## Chunk 3 - MapKit Tile Overlay MVP

Purpose: render the backend PNG tile set over an Apple map.

Deliverables:

- `MKTileOverlay` configuration from the fetched manifest `tile_url_template`.
- Overlay lifecycle that replaces stale overlays when the selected tile set changes.
- Zoom and bounds guards based on manifest metadata.
- Loading, empty, unavailable, and retry states.
- A simple opacity control for the dark-sky overlay.

Verification:

- A local smoke run shows the latest tile overlay over MapKit when the backend has a published manifest.
- Tests cover URL construction and overlay state transitions without requiring networked map rendering.

## Chunk 4 - Legend, Dataset Metadata, And Product Copy

Purpose: make the rendered color classes understandable without overstating scientific meaning.

Deliverables:

- Legend generated from `GET /api/v1/tiles/classes`.
- Dataset metadata panel showing dataset date, classification version, render version, tile count, and source claim text.
- Product copy that explains transparent pixels as nodata/rejected source data.
- Accessibility labels for legend classes and map controls.

Verification:

- UI tests or snapshot tests cover legend ordering and empty/error states.
- Product copy avoids Bortle, SQM, and certified observing-site claims.

## Chunk 5 - Tile Set Selection And Freshness UX

Purpose: let users inspect available backend tile sets and switch between them.

Deliverables:

- Paginated tile-set list using `meta.next_cursor`.
- Latest tile-set indicator.
- Selection persistence across app launches.
- Refresh action that checks for a newer latest manifest.

Verification:

- Tests cover pagination, selection restoration, stale selected tile-set handling, and latest refresh behavior.

## Chunk 6 - Offline Cache And Resilience

Purpose: keep the app useful across intermittent network conditions without storing excessive map data.

Deliverables:

- Small persistent cache for latest manifest, classes, and selected tile-set summary.
- Use of URL loading cache for PNG tiles with backend cache headers.
- Explicit offline state when manifest/class metadata cannot refresh.
- Cache invalidation when classification version or tile-set id changes.

Verification:

- Unit tests cover cache read/write, invalidation, and offline fallback.
- App does not silently present stale data as fresh.

## Chunk 7 - Native Polish And Platform Adaptation

Purpose: make the shared app feel native on iPhone, iPad, Mac, and Apple Vision Pro.

Deliverables:

- Platform-appropriate sidebars, inspectors, menus, keyboard shortcuts, and toolbar actions.
- Location permission flow for optional map centering.
- Map controls for user location, scale, compass, overlay opacity, and dataset picker.
- Deep-link shape for opening a coordinate or tile-set selection.

Verification:

- Manual platform checklist covers iPhone, iPad, macOS window resizing, and visionOS windowed presentation.
- Permission-denied location flow still allows manual map browsing.

## Chunk 8 - App Quality Gates And CI

Purpose: make app validation reproducible locally and in CI.

Deliverables:

- Formatting/linting choice documented and wired only if the repository adopts it.
- Unit tests for API models, backend client behavior, caches, and map state reducers.
- UI smoke tests for launch, map screen, legend, and tile-set picker.
- CI build/test jobs for iOS simulator, macOS, and visionOS simulator where available.

Verification:

- Local app validation command builds and tests supported targets.
- CI runs the same meaningful checks without requiring private backend credentials.

## Chunk 9 - Local Launch Readiness

Purpose: prepare the first internal app build against the local/backend product.

Deliverables:

- Developer guide for running backend plus app.
- App configuration documentation.
- Privacy and data-use notes.
- Known limitations list for tile availability, science claims, and offline behavior.

Verification:

- A clean-clone developer can start the backend, launch the app, and view a published tile set.
- Status and gap register reflect remaining work.
