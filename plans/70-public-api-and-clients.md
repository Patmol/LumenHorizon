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
| `GET /api/v1/tiles/manifest/{manifest_id}` | Specific tile manifest. |
| `GET /api/v1/tile-sets` | Paginated tile-set metadata. |
| `GET /api/v1/tile-classes` | Classification metadata. |
| `GET /api/v1/tiles/{classification}/{z}/{x}/{y}.png` | Tile redirect. |

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
- Product copy must describe `radiance-dark-sky-v1` as VIIRS radiance-based evidence, not measured observing quality.

## Verification

- `just openapi-check` validates implemented `/api/v1` route inventory and auth posture.
- Gateway tests cover request hardening, route envelopes, auth errors, rate-limit responses, tile redirects, and admin route policies.
- API docs must be updated with route or envelope changes.
