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
