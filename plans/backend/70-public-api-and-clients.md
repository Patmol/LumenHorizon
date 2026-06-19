# 70. Public API And Clients

The active API plan describes local API Gateway behavior and client contracts. Public hosting is not part of the active plan set.

## API Gateway Responsibilities

- Serve `/api/v1` product and admin routes.
- Add request IDs and security headers.
- Keep CORS disabled unless explicitly configured.
- Validate admin JWTs through configured issuer, audience, JWKS URL, optional tenant id, and role claim.
- Apply route-class rate limits.
- Return sanitized envelopes.
- Keep OpenAPI aligned with implemented routes.

## Product Routes

| Route | Purpose |
| --- | --- |
| `GET /api/v1/tiles/manifest` | Latest tile manifest. |
| `GET /api/v1/tiles/manifest/{tile_set_id}` | Specific immutable tile manifest. |
| `GET /api/v1/tiles/sets` | Paginated tile-set metadata. |
| `GET /api/v1/tiles/classes` | Classification metadata. |
| `GET /api/v1/tiles/{tile_set_id}/{z}/{x}/{y}.png` | Tile redirect. |

## Deferred Public Site Routes

The gateway reserves these anonymous route contracts for the future observing-site feature. They validate request shape, are represented in OpenAPI, and currently return sanitized `503 service_unavailable` responses until site data ownership, import workflows, and score semantics are promoted from [future/observing-sites-and-sky-quality.md](future/observing-sites-and-sky-quality.md).

| Route | Current behavior |
| --- | --- |
| `GET /api/v1/sites` | Validates search parameters, then returns deferred unavailable. |
| `GET /api/v1/sites/{site_id}` | Validates UUID site id, then returns deferred unavailable. |
| `GET /api/v1/sites/{site_id}/score` | Validates UUID site id, then returns deferred unavailable. |

## Admin Routes

| Route | Purpose |
| --- | --- |
| `GET /api/v1/admin/ingest/runs` | List ingest runs. |
| `GET /api/v1/admin/processing/runs` | List processing runs. |
| `GET /api/v1/admin/health/deep` | Deep dependency status. |
| `POST /api/v1/admin/ingest/trigger` | Trigger local/internal ingest through the gateway. |
| `POST /api/v1/admin/processing/requeue` | Requeue a processing item. |

## Client Contract Rules

- `/api/v1` permits additive compatible changes.
- Breaking response shape, semantics, auth, or pagination changes require a new versioned path.
- Cursors are opaque.
- Clients consume manifests and should not infer storage paths.
- Tile URL templates use `{z}`, `{x}`, and `{y}` placeholders.
- Tile redirect paths use immutable `{tile_set_id}` values, not classification names.
- Product copy must describe `radiance-dark-sky-v1` as VIIRS radiance-based evidence, not measured observing quality.

## Verification

- `just openapi-check` validates implemented `/api/v1` route inventory and auth posture.
- Gateway tests cover request hardening, route envelopes, auth errors, rate-limit responses, tile redirects, and admin route policies.
- API guide examples cover latest manifest fetch, tile-set pagination via `meta.next_cursor`, and tile redirect URL substitution against a local gateway.
- API docs must be updated with route or envelope changes.
