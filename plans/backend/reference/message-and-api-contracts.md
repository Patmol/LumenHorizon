# Message And API Contracts Reference

Contracts in this file should remain backward-compatible once production consumers exist. Breaking changes require versioning.

## Storage Containers

| Container | Purpose |
|-----------|---------|
| `raw-viirs` | Raw downloaded HDF5 granules from NASA Earthdata. |
| `processed-tiles` | Rendered map tiles, manifests, and tile metadata products. |
| `user-uploads` | Reserved for future user-provided data. |

Blob names are paths inside their containers. Do not include container names in blob paths.

## Legacy Blob Path Compatibility

Decision: reject blob paths that start with `raw-viirs/`.

This repository is following a clean-start roadmap and does not carry production legacy raw blob records. New ingest rows and queue messages must store paths relative to the `raw-viirs` container, such as `VNP46A2/2026-05-21/h11v06.h5`. Readers and message builders should treat container-prefixed paths as invalid instead of silently stripping the prefix, because accepting both forms would make queue and manifest contracts ambiguous before production consumers exist.

If real legacy rows are introduced later, add an explicit migration or a versioned compatibility reader before enabling production processing. Do not normalize prefixed paths ad hoc at call sites.

## Queues

| Queue | Purpose |
|-------|---------|
| `viirs-processing` | Processing work emitted by `ingest-svc`. |
| `viirs-processing-deadletter` | Poison or invalid processing messages. |

## Processing Message

Implementation note: this contract is produced by `ingest-svc` and consumed by `processing-svc`, so the Rust DTO and validation logic should live in the narrow `backend/shared` crate once that crate is introduced. Service-specific enqueue, receive, delete, retry, and dead-letter behavior remains outside the shared crate.

Queue messages are JSON:

```json
{
  "ingest_id": "uuid",
  "blob_path": "VNP46A2/2026-05-21/h11v06.h5",
  "product": "VNP46A2",
  "granule_date": "2026-05-21T00:00:00Z",
  "tile_h": 11,
  "tile_v": 6
}
```

Validation rules:

- `ingest_id` parses as UUID.
- `blob_path` is relative to `raw-viirs` and does not begin with `/`, `http://`, `https://`, or `raw-viirs/`.
- `product` is supported by product dataset map.
- `tile_h` and `tile_v` match the first `hXXvYY` pattern in `blob_path`.
- `granule_date` parses as RFC3339 UTC or UTC-equivalent timestamp.

Add new fields only in a backward-compatible way. Version the message for breaking changes.

## Tile Manifest

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
  "source_granules": [
    {
      "ingest_id": "uuid",
      "product": "VNP46A2",
      "blob_path": "VNP46A2/2026-05-21/h11v06.h5"
    }
  ],
  "checksums": {
    "manifest_sha256": "hex"
  }
}
```

The Apple client fetches a manifest first, then configures `MKTileOverlay` with `tile_url_template`.

Current Chunk 9 processing-generated tile-set IDs use `{dataset_date}-{classification_version}-{ingest_id_prefix}-a{attempt}`. Blob paths remain container-relative and must not include `processed-tiles/`.

Retention cleanup may delete whole stale tile-set outputs after the processed tile-set retention window, but it must not overwrite immutable tile blobs or immutable manifests in place. The mutable `manifests/latest.json` pointer is never deleted by cleanup and must point only to a retained immutable manifest.

Public clients should treat `classification_version: "radiance-dark-sky-v1"` as a VIIRS radiance-based dark-sky class. It is not a measured Bortle class, SQM reading, or certified observing-site quality score.

## API Response Envelope

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
    "message": "radius_km must be between 1 and 250",
    "details": {
      "field": "radius_km"
    }
  }
}
```

List responses include `next_cursor` in `meta`.

## Error Codes

| HTTP | Code | Meaning |
|------|------|---------|
| 400 | `invalid_request` | Malformed JSON, invalid query parameter, or validation failure. |
| 401 | `unauthenticated` | Missing, invalid, or expired token. |
| 403 | `forbidden` | Authenticated subject lacks permission. |
| 404 | `not_found` | Resource does not exist or is not visible. |
| 409 | `conflict` | Request conflicts with resource state. |
| 422 | `unprocessable_entity` | Valid JSON shape but semantically invalid domain input. |
| 429 | `rate_limited` | Rate limit exceeded. |
| 500 | `internal_error` | Unexpected server failure. |
| 502 | `upstream_error` | Internal dependency failed. |
| 503 | `service_unavailable` | Required dependency unavailable or service not ready. |
| 503 | `tile_unavailable` | Tile set exists but requested output is unavailable. |

Error messages must not reveal secrets, connection strings, internal stack traces, or raw upstream credentials.