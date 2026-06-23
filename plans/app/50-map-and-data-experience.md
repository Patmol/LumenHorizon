# 50. Map And Data Experience

The app's primary experience is a MapKit map with the backend's PNG light-pollution/dark-sky tile set rendered as an overlay.

## Map Screen

Initial map screen responsibilities:

- Load latest manifest and class metadata.
- Show a MapKit base map with the dark-sky tile overlay.
- Provide overlay opacity control.
- Provide legend access.
- Show dataset date and freshness.
- Show clear loading, unavailable, offline, and retry states.

## Tile Overlay Behavior

- Fetch the latest manifest before creating the overlay.
- Validate `format == "png"`, positive `tile_size`, valid bounds, valid zoom ranges, and a URL template containing `{z}`, `{x}`, and `{y}`.
- Configure `MKTileOverlay` from the validated `tile_url_template`.
- Treat transparent PNG pixels as no-data or rejected source data.
- Replace the overlay when the selected immutable `tile_set_id` changes.
- Avoid requesting or implying coverage outside manifest bounds.

## Map Bounds And Zoom

| Manifest field | App behavior |
| --- | --- |
| `bounds` | Bound the active overlay camera to published coverage and explain areas outside coverage. |
| `min_zoom` | Farthest active overlay camera zoom. |
| `max_native_zoom` | Maximum native tile request zoom. |
| `max_display_zoom` | Closest active overlay camera zoom before tiles are over-scaled. |

MapKit may still render base-map content outside tile coverage before a manifest is loaded or when no overlay is active. When a dark-sky overlay is active, camera zoom and panning are constrained from manifest metadata so the visible map does not imply overlay coverage outside the published tile set.

## Legend

The legend is generated from `GET /api/v1/tiles/classes`:

- `classification_version` confirms the classes match the selected manifest.
- `radiance_units` labels the radiance ranges.
- `classes[].class`, `label`, `color_hex`, `min_radiance`, and `max_radiance_exclusive` drive legend rows.

The app should not hardcode class labels or colors except in fixtures used by tests and previews.

## Dataset Metadata

The dataset panel should show:

- Dataset date.
- Generated timestamp.
- Classification version.
- Render version.
- Tile count.
- Native/display zoom range.
- Bounds summary.
- Claim-language disclaimer.

Suggested copy:

> This layer shows VIIRS radiance-based dark-sky evidence from LumenHorizon processing. It is not a measured Bortle class, SQM reading, or certified observing-site score.

## Tile Set Selection

The first app version can default to the latest tile set. A later chunk adds:

- Paginated tile-set list.
- Latest indicator.
- Dataset date grouping.
- Selected tile-set persistence.
- Refresh action to check for newer latest output.

## Error And Empty States

| State | UX |
| --- | --- |
| No manifest | Explain that no tile set is currently published. |
| Manifest unavailable | Show retry and backend base URL diagnostics. |
| Classes unavailable | Render map if manifest is valid, but show legend unavailable. |
| Selected tile set missing | Fall back to latest only after telling the user the saved selection is unavailable. |
| Offline with cache | Label cached metadata with last successful refresh time. |
| Outside bounds | Allow browsing but explain no overlay data exists for the region. |

## Location

Location is optional. If enabled, it only centers the map on the user's current location. The app should not store a location history or require location access to browse the map.
