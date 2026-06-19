# 60. Backend Integration

The app consumes anonymous product routes from `api-gateway`. It does not call admin routes and does not need backend credentials for the first version.

## Routes

| Route | App use |
| --- | --- |
| `GET /api/v1/tiles/manifest` | Load the latest tile manifest before creating the overlay. |
| `GET /api/v1/tiles/manifest/{tile_set_id}` | Reload a saved or selected immutable tile set. |
| `GET /api/v1/tiles/sets` | List available tile sets with opaque cursor pagination. |
| `GET /api/v1/tiles/classes` | Populate legend labels, colors, radiance ranges, and claim metadata. |
| `GET /api/v1/tiles/{tile_set_id}/{z}/{x}/{y}.png` | Documented redirect route if the app needs gateway-mediated tile URLs. |

The backend manifest includes `tile_url_template`, which is the preferred MapKit template when present and valid. The app should not infer blob paths or storage hostnames.

## Client Responsibilities

- Add a stable user agent identifying app name, platform, version, and build when practical.
- Use explicit timeouts for metadata requests.
- Decode the backend envelope and route-specific `data`.
- Preserve backend `request_id` for diagnostics.
- Respect backend cache headers for metadata and PNG tile requests.
- Surface sanitized backend errors without exposing internal implementation details.

## Tile URL Template Handling

Validation rules before creating `MKTileOverlay`:

- URL parses as absolute `http` or `https`.
- Template contains `{z}`, `{x}`, and `{y}` exactly as placeholders.
- Manifest `format` is `png`.
- Manifest zoom range is ordered: `min_zoom <= max_native_zoom <= max_display_zoom`.
- Bounds are valid geographic coordinates.

If the template is invalid, the app should report the tile set as unavailable and avoid creating the overlay.

## API Envelope Handling

Success responses:

```json
{
  "data": {},
  "meta": {
    "request_id": "uuid",
    "timestamp": "2026-05-21T09:00:00Z"
  },
  "error": null
}
```

Failure responses:

```json
{
  "data": null,
  "meta": {
    "request_id": "uuid",
    "timestamp": "2026-05-21T09:00:00Z"
  },
  "error": {
    "code": "service_unavailable",
    "message": "tile manifest storage is not configured",
    "details": null
  }
}
```

The client should map known error codes to user-facing states and preserve unknown codes as generic backend errors with retry guidance when appropriate.

## Pagination

Tile-set list responses include `meta.next_cursor` when another page exists. Cursors are opaque and must not be parsed by the app.

## Caching

Suggested metadata cache policy:

- Latest manifest: short-lived cache, refresh on app foreground and manual refresh.
- Immutable manifest by id: cache longer because tile-set ids are immutable.
- Tile classes: cache by `classification_version`.
- Tile-set list pages: cache only as a convenience; refresh before presenting as authoritative.

PNG tile caching should initially rely on `URLCache` and backend cache headers. A separate offline map plan is required before adding explicit bulk tile downloads.

## Local Development Integration

Debug builds can point at the local API Gateway. A local smoke run needs:

1. Backend dependencies running.
2. API Gateway serving.
3. A latest manifest published by processing.
4. App debug API base URL configured for the gateway.

If no latest manifest is available, the app should still launch and show a clear no-data state.
