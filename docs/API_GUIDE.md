# API Guide

This guide describes the active local HTTP API surface for LumenHorizon. The API Gateway is the preferred entry point for product and admin routes; `ingest-svc` still exposes internal development routes used by local workflows.

## Base URLs

Start local services with:

```bash
just serve
just serve-api
```

Then set:

```bash
export LUMENHORIZON_INGEST_URL="http://localhost:${PORT:-8083}"
export LUMENHORIZON_API_URL="http://localhost:${API_GATEWAY_PORT:-8080}"
```

## Response Envelope

Gateway `/api/v1` responses use a stable envelope:

```json
{
  "data": {},
  "meta": {
    "request_id": "..."
  },
  "error": null
}
```

Errors keep `data` null and return a sanitized `error` object with a stable code and message.
List responses put an opaque `next_cursor` in `meta` when another page is available:

```json
{
  "data": [],
  "meta": {
    "request_id": "...",
    "next_cursor": "opaque-cursor"
  },
  "error": null
}
```

## Ingest Service Development Routes

### Liveness

```bash
curl --fail "$LUMENHORIZON_INGEST_URL/health"
```

Returns:

```json
{ "status": "healthy" }
```

### Readiness

```bash
curl --fail "$LUMENHORIZON_INGEST_URL/ready"
```

Readiness checks PostgreSQL, raw blob storage, and processing queue access.

### Trigger Ingest

```bash
curl --fail -X POST "$LUMENHORIZON_INGEST_URL/admin/ingest/trigger"
```

This route is for local/internal development. Use gateway admin routes when testing authenticated admin behavior.

## Gateway Routes

### Liveness

```bash
curl --fail "$LUMENHORIZON_API_URL/health"
```

### Readiness

```bash
curl --fail "$LUMENHORIZON_API_URL/ready"
```

Readiness reports gateway configuration, auth configuration, rate-limit store readiness, database wiring, tile manifest storage, processing queue wiring, and ingest-service admin wiring. Local mode can run without every backed dependency configured.

### Latest Tile Manifest

```bash
curl --fail "$LUMENHORIZON_API_URL/api/v1/tiles/manifest"
```

Returns the latest published tile manifest from the processed tiles container. If tile storage is not configured or no latest manifest exists, the route returns a sanitized unavailable or not-found error.

### Tile Manifest By ID

```bash
curl --fail "$LUMENHORIZON_API_URL/api/v1/tiles/manifest/<tile-set-id>"
```

### Tile Sets

```bash
curl --fail "$LUMENHORIZON_API_URL/api/v1/tiles/sets?limit=20"
```

List responses include `next_cursor` in `meta` when another page exists. Cursors are opaque.

### Tile Redirect

```bash
curl -i "$LUMENHORIZON_API_URL/api/v1/tiles/<tile-set-id>/<z>/<x>/<y>.png"
```

Valid coordinates redirect to the tile object URL from the manifest. Invalid coordinates return `400 invalid_request`; valid-but-missing tiles return `404 tile_not_found`.

### Tile Classes

```bash
curl --fail "$LUMENHORIZON_API_URL/api/v1/tiles/classes"
```

Returns available classification metadata, including the current `radiance-dark-sky-v1` product claim language.

## Client Workflow Example

The following examples use anonymous product routes only. Set `LUMENHORIZON_API_URL` to the local gateway URL before running them:

```bash
export LUMENHORIZON_API_URL="http://localhost:${API_GATEWAY_PORT:-8080}"
```

Fetch the latest manifest and read the immutable tile-set id plus tile URL template:

```bash
manifest_json="$(curl --fail "$LUMENHORIZON_API_URL/api/v1/tiles/manifest")"
tile_set_id="$(jq -r '.data.tile_set_id' <<<"$manifest_json")"
tile_url_template="$(jq -r '.data.tile_url_template' <<<"$manifest_json")"
```

The manifest `data` object includes:

| Field | Client use |
| --- | --- |
| `tile_set_id` | Immutable tile-set id used in manifest and redirect paths. |
| `min_zoom`, `max_native_zoom`, `max_display_zoom` | Zoom range clients should request. |
| `bounds` | Geographic extent for tile availability checks. |
| `tile_url_template` | Storage/CDN URL template with `{z}`, `{x}`, and `{y}` placeholders. |
| `tile_count` | Number of generated native tiles in the manifest. |
| `checksums.manifest_sha256` | Manifest integrity evidence. |

Walk tile sets by following `meta.next_cursor` until it is absent:

```bash
cursor=""
while :; do
  url="$LUMENHORIZON_API_URL/api/v1/tiles/sets?limit=5"
  if [[ -n "$cursor" ]]; then
    url="$url&cursor=$cursor"
  fi

  page_json="$(curl --fail "$url")"
  jq '.data[] | {tile_set_id, dataset_date, latest}' <<<"$page_json"
  cursor="$(jq -r '.meta.next_cursor // empty' <<<"$page_json")"
  [[ -n "$cursor" ]] || break
done
```

Request tiles through the gateway using the immutable tile-set id. A valid tile returns `302` with a `Location` header built from `tile_url_template`:

```bash
curl -i "$LUMENHORIZON_API_URL/api/v1/tiles/$tile_set_id/3/1/2.png"
```

Common product-route errors are `400 invalid_request` for malformed parameters or impossible tile coordinates, `404 tile_not_found` for valid coordinates outside manifest bounds, `429 rate_limited`, and `503 service_unavailable` when a backing dependency or manifest is unavailable.

### Deferred Observing Site Routes

The gateway reserves the future observing-site route shapes in OpenAPI. These routes validate request shape and currently return sanitized unavailable responses until site data and score semantics are promoted:

```bash
curl -i "$LUMENHORIZON_API_URL/api/v1/sites?lat=40&lon=-105&radius_km=50"
curl -i "$LUMENHORIZON_API_URL/api/v1/sites/00000000-0000-0000-0000-000000000001"
curl -i "$LUMENHORIZON_API_URL/api/v1/sites/00000000-0000-0000-0000-000000000001/score"
```

## Admin Routes

Admin routes live under `/api/v1/admin` and require a valid RS256 JWT with the configured admin role claim and role value.

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/api/v1/admin/ingest/runs` | List ingest runs. |
| `GET` | `/api/v1/admin/processing/runs` | List processing runs. |
| `GET` | `/api/v1/admin/health/deep` | Return deeper dependency status. |
| `POST` | `/api/v1/admin/ingest/trigger` | Trigger ingest through the ingest service. |
| `POST` | `/api/v1/admin/processing/requeue` | Requeue a processing item. |

Backed admin routes may require `DATABASE_URL`, storage variables, `AZURE_QUEUE_NAME`, `INGEST_SERVICE_BASE_URL`, and `INTERNAL_SERVICE_AUTH_TOKEN` depending on the route.

## Security Behavior

The gateway applies:

- request IDs through `x-request-id`
- security headers
- disabled CORS unless explicitly configured
- request size, URL length, method, and path hardening
- route-class rate limiting with memory or Redis-compatible stores
- sanitized error envelopes
- admin JWT/JWKS validation
- admin route-to-role policy checks
- admin audit logging without request bodies or secrets

Run the API contract check with:

```bash
just openapi-check
```

The contract source is [backend/api-gateway/openapi/openapi.yaml](../backend/api-gateway/openapi/openapi.yaml).
