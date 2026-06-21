# API And Data Contracts Reference

This app reference summarizes the backend contracts consumed by the native Apple client. Backend remains the source of truth; if these contracts change, update backend docs and app fixtures together.

## API Base

The app calls anonymous product routes under:

```text
{api_base_url}/api/v1
```

The first app version does not call admin routes and does not require an access token.

## API Envelope

Success:

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

Failure:

```json
{
  "data": null,
  "meta": {
    "request_id": "uuid",
    "timestamp": "2026-05-21T09:00:00Z"
  },
  "error": {
    "code": "invalid_request",
    "message": "tile zoom is outside tile set range",
    "details": null
  }
}
```

Known error codes are documented in [../../backend/reference/message-and-api-contracts.md](../../backend/reference/message-and-api-contracts.md). The app should handle unknown codes as generic backend errors.

## Tile Manifest

Manifest routes:

```text
GET /api/v1/tiles/manifest
GET /api/v1/tiles/manifest/{tile_set_id}
```

Required fields for the app:

| Field | App use |
| --- | --- |
| `tile_set_id` | Immutable overlay identity and saved selection key. |
| `dataset_date` | Dataset label and tile-set picker grouping. |
| `generated_at` | Freshness label. |
| `classification_version` | Match against tile class metadata. |
| `render_version` | Diagnostics and metadata panel. |
| `format` | Must be `png` for the first app version. |
| `tile_size` | Tile overlay configuration and validation. |
| `min_zoom` | Minimum supported tile zoom. |
| `max_native_zoom` | Maximum native tile request zoom. |
| `max_display_zoom` | Maximum useful display zoom. |
| `bounds` | Coverage extent. |
| `tile_url_template` | URL template for `MKTileOverlay`. |
| `tile_count` | Metadata panel and diagnostics. |
| `checksums.manifest_sha256` | Integrity evidence display/diagnostics. |

Example:

```json
{
  "tile_set_id": "2026-05-21-radiance-dark-sky-v1-00000000-a1",
  "dataset_date": "2026-05-21",
  "generated_at": "2026-05-21T09:15:00Z",
  "classification_version": "radiance-dark-sky-v1",
  "render_version": "tiles-v1",
  "processor_version": "processing-svc:git-sha",
  "format": "png",
  "tile_size": 256,
  "min_zoom": 3,
  "max_native_zoom": 10,
  "max_display_zoom": 12,
  "bounds": {
    "west": -125.0,
    "south": 24.0,
    "east": -66.0,
    "north": 50.0
  },
  "tile_url_template": "https://tiles.lumenhorizon.com/tiles/2026-05-21-radiance-dark-sky-v1-00000000-a1/{z}/{x}/{y}.png",
  "tile_count": 12345,
  "source_granules": [],
  "checksums": {
    "manifest_sha256": "hex"
  }
}
```

## Tile Classes

Route:

```text
GET /api/v1/tiles/classes
```

Current shape:

```json
{
  "classification_version": "radiance-dark-sky-v1",
  "radiance_units": "nW/cm^2/sr",
  "classes": [
    {
      "class": 1,
      "color_hex": "#05070d",
      "label": "Excellent dark site",
      "min_radiance": 0.0,
      "max_radiance_exclusive": 0.2
    }
  ]
}
```

The app uses this route for legend colors and labels. It should not hardcode class labels or colors in production code.

## Tile Sets

Route:

```text
GET /api/v1/tiles/sets?limit=20&cursor=<opaque>
```

The app uses tile-set summaries for historical selection. List responses include `meta.next_cursor` when another page exists. Cursors are opaque and must not be parsed.

Each list item is a tile-set summary. It mirrors the manifest's identity and zoom/bounds metadata but omits `tile_url_template` and `checksums`; fetch the full manifest by id when those are needed.

| Field | App use |
| --- | --- |
| `tile_set_id` | Immutable overlay identity and saved selection key. |
| `dataset_date` | Dataset label and tile-set picker grouping. |
| `classification_version` | Match against tile class metadata. |
| `render_version` | Diagnostics and metadata panel. |
| `format` | Must be `png` for the first app version. |
| `min_zoom`, `max_native_zoom`, `max_display_zoom` | Zoom range guards. |
| `bounds` | Coverage extent. |
| `tile_count` | Metadata panel and diagnostics. |
| `latest` | Latest tile-set indicator in the picker. |
| `created_at` | Freshness/sort ordering. |

## PNG Tiles

The manifest `tile_url_template` provides the preferred tile URL source for MapKit:

```text
https://.../{z}/{x}/{y}.png
```

The API Gateway also exposes a documented redirect route:

```text
GET /api/v1/tiles/{tile_set_id}/{z}/{x}/{y}.png
```

The app should prefer the manifest template and use the redirect route only when a deliberate gateway-mediated tile-loading mode is added.

## Product Claim Rule

The app must describe `radiance-dark-sky-v1` as VIIRS radiance-based dark-sky evidence. It must not present the current backend product as measured Bortle class, SQM data, certified observing quality, or observing-site score.
