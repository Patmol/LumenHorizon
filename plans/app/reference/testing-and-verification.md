# Testing And Verification

## Draft Local App Checks

The exact commands should be finalized after the Xcode project is created.

```bash
xcodebuild -scheme LumenHorizon-iOS -destination 'platform=iOS Simulator,name=iPhone 16' build test
xcodebuild -scheme LumenHorizon-macOS -destination 'platform=macOS' build test
xcodebuild -scheme LumenHorizon-visionOS -destination 'platform=visionOS Simulator,name=Apple Vision Pro' build test
```

The visionOS check requires an installed visionOS SDK and simulator runtime. If unavailable, record the skipped check and reason in the chunk handoff.

If a shared Swift package exists:

```bash
swift test
```

## Contract Fixture Checks

App tests should include fixtures for:

- Latest tile manifest success envelope.
- Tile manifest by id success envelope.
- Tile classes success envelope.
- Tile-set list with and without `meta.next_cursor`.
- Backend failure envelopes for `invalid_request`, `not_found`, `tile_unavailable`, `tile_not_found`, and `service_unavailable`.
- Malformed manifest data that must not create a tile overlay.

Fixtures should be copied from backend docs or backend route tests when practical.

## Unit Test Focus Areas

- `ApiEnvelope` decoding.
- Backend error mapping.
- Tile manifest validation.
- Tile URL template placeholder substitution.
- Tile classes decoding and legend ordering.
- Tile-set pagination.
- Selected tile-set persistence.
- Metadata cache freshness and invalidation.
- Map overlay state transitions.

## UI Smoke Focus Areas

- App launches without a configured backend and shows setup/no-data guidance.
- App loads fixture manifest/classes and shows map, legend, and metadata.
- Overlay opacity control updates state.
- Tile-set picker handles loading, empty, error, and selected states.
- macOS sidebar/inspector, iOS navigation, and visionOS windowed presentation adapt correctly.

## Local Backend Smoke

When a local backend has a latest manifest:

```bash
just up
just migrate
just serve-api
```

Then run the app with `api_base_url` pointing at the local API Gateway and verify:

1. Latest manifest loads.
2. Tile classes load.
3. MapKit requests PNG tiles through the manifest template.
4. Legend and metadata match the loaded manifest.
5. The app remains usable if the backend is stopped after metadata has been cached, with stale/offline state clearly labeled.

## Evidence Standard

A completed app chunk should state which app checks ran and whether any skipped checks require follow-up. Checks that depend on Xcode versions, simulator runtimes, or a seeded backend manifest should document those prerequisites.
