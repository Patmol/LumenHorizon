# 10. Product And Architecture

## Product Goal

The LumenHorizon app helps users explore broad light-pollution and dark-sky patterns on iOS, macOS, and visionOS by rendering the backend's VIIRS radiance-based PNG tiles over MapKit. The first version is a read-only map client for backend-published tile sets.

## Non-Goals

- Do not claim measured Bortle class, SQM readings, certified observing quality, or observing-site scores from the current radiance product.
- Do not perform VIIRS processing, classification, or tile generation in the app.
- Do not require a user account or admin API access for the first read-only map client.
- Do not infer storage paths or tile URLs from backend internals. Consume manifest and API contracts.
- Do not build a browser/web client as part of this app plan.

## Supported Platforms

| Platform | Initial support |
| --- | --- |
| iOS | Primary mobile target with touch-first map browsing. |
| iPadOS | Uses the iOS target with adaptive navigation and larger-screen layout. |
| macOS | Native SwiftUI target with MapKit, sidebar/inspector layout, menus, and keyboard shortcuts. |
| visionOS | Native SwiftUI target with MapKit in a windowed spatial experience; immersive or volumetric map experiences are deferred unless promoted by a separate plan. |
| watchOS/tvOS | Out of scope unless promoted by a separate plan. |

## App Components

| Component | Responsibility |
| --- | --- |
| `LumenHorizonApp` | App entry points, scenes, dependency construction, and platform-specific shell. |
| `AppCore` | Shared models, app state, configuration, cache policy, and feature reducers/view models. |
| `BackendClient` | Anonymous API calls, envelope decoding, backend error mapping, and request IDs. |
| `MapFeature` | MapKit region state, tile overlay lifecycle, tile-set selection, opacity, and bounds handling. |
| `TileCatalogFeature` | Latest manifest, available tile sets, tile classes, and refresh state. |
| `Persistence` | Small durable cache for selected tile set, latest manifest metadata, classes, and user preferences. |
| `DesignSystem` | Shared colors, legend presentation, copy components, accessibility helpers, and platform-adaptive controls. |

The exact module shape can be a Swift package, multiple app targets with shared source folders, or a hybrid. The important boundary is that networking, DTOs, map state, and persistence remain testable outside concrete SwiftUI views.

## Runtime Data Flow

```text
LumenHorizon app
  -> api-gateway GET /api/v1/tiles/manifest
  -> api-gateway GET /api/v1/tiles/classes
  -> optional GET /api/v1/tiles/sets for historical tile sets
  -> MapKit MKTileOverlay requests PNG tiles from manifest tile_url_template
```

The backend manifest is the source of truth for tile identity, bounds, zoom range, format, and URL template. The class metadata endpoint is the source of truth for legend labels, colors, radiance units, and classification version.

## Map Architecture

- Use SwiftUI for app structure and wrap `MKMapView` when `MKTileOverlay` lifecycle control is more precise than the native SwiftUI `Map` API.
- Keep `MKTileOverlay` creation in a small adapter that accepts a validated `TileManifest`.
- Configure overlay replacement by immutable `tile_set_id`; do not mutate an existing overlay when switching data.
- Use manifest `bounds` to avoid misleading empty coverage outside the tile set extent.
- Use manifest `max_native_zoom` for tile requests and `max_display_zoom` for presentation guidance.
- Render transparent backend pixels as no overlay content; do not synthesize colors client-side.
- Keep the first visionOS experience windowed and adaptive; do not require immersive spaces for core map browsing.

## Configuration Model

| Configuration | Purpose |
| --- | --- |
| API base URL | Base gateway URL for anonymous `/api/v1` routes. |
| Request timeout | Upper bound for manifest/classes/tile-set list requests. |
| Cache policy | Controls metadata cache TTL and offline fallback behavior. |
| Default map region | Initial camera region before a manifest is loaded. |
| Debug fixture mode | Optional local fixtures for previews and tests only. |

Release builds must not point at a local developer URL by default. Debug builds can default to the local API Gateway when the backend developer guide documents the port.

## Design Rules

- Treat backend API envelopes and tile manifests as versioned contracts.
- Keep product claim language tied to backend classification metadata.
- Prefer strict validation for manifest shape before creating a MapKit overlay.
- Keep secrets out of app configuration; anonymous product routes should not require tokens.
- Keep map state deterministic and unit-testable outside MapKit rendering.
- Keep platform-specific UI thin and shared behavior in app core modules.
