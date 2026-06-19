# Configuration Reference

Keep `.env.example`, service config structs, scripts, and this file aligned.

## Shared Local Variables

| Variable | Required | Default | Purpose |
| --- | --- | --- | --- |
| `DATABASE_URL` | yes | none | PostgreSQL connection for local services. |
| `RUST_LOG` | no | service-specific | Tracing filter. |
| `HTTP_REQUEST_TIMEOUT_SECONDS` | no | `30` | Timeout for outbound CMR, Earthdata, blob, and queue HTTP requests. |
| `HTTP_RETRY_MAX_ATTEMPTS` | no | `3` | Maximum attempts for retryable HTTP operations. |
| `HTTP_RETRY_BASE_DELAY_MS` | no | `250` | Retry base delay. |
| `HTTP_RETRY_MAX_DELAY_MS` | no | `5000` | Retry max delay. |

## Storage Variables

The current runtime uses Azurite-compatible Blob and Queue REST APIs.

| Variable | Required | Default | Purpose |
| --- | --- | --- | --- |
| `AZURE_STORAGE_ACCOUNT` | yes for ingest/processing; optional for some gateway routes | `devstoreaccount1` locally | Storage account name. |
| `AZURE_STORAGE_ACCESS_KEY` | yes when storage is configured | Azurite dev key locally | Storage key for blob and queue access. |
| `AZURE_STORAGE_EMULATOR_HOST` | no | `127.0.0.1` locally | Azurite host. |
| `AZURE_QUEUE_NAME` | no | `viirs-processing` | Processing queue. |
| `AZURE_DEADLETTER_QUEUE_NAME` | no | `viirs-processing-deadletter` | Dead-letter queue. |
| `RAW_VIIRS_CONTAINER` | no | `raw-viirs` | Raw granule container. |
| `PROCESSED_TILES_CONTAINER` | no | `processed-tiles` | Tile artifact container. |

## Ingest Variables

| Variable | Required | Default | Purpose |
| --- | --- | --- | --- |
| `PORT` | no | `8083` | `ingest-svc` listener port. |
| `EARTHDATA_BEARER_TOKEN` | yes for authenticated downloads | none | NASA Earthdata bearer token. |
| `CMR_BASE_URL` | no | CMR default | CMR endpoint. |
| `BOUNDING_BOX` | no | product default | Search bounding box. |
| `INGEST_MAX_GRANULES` | no | unset | Optional per-run limit. |
| `INTERNAL_SERVICE_AUTH_TOKEN` | no | unset | Enables internal admin auth on ingest admin routes. |
| `INTERNAL_SERVICE_AUTH_HEADER` | no | `x-lumenhorizon-internal-token` | Header for internal admin auth. |

## Processing Variables

| Variable | Required | Default | Purpose |
| --- | --- | --- | --- |
| `PROCESSING_VISIBILITY_TIMEOUT_SECONDS` | no | `300` | Queue invisibility window during processing. |
| `PROCESSING_MAX_DEQUEUE_COUNT` | no | `5` | Dead-letter threshold. |
| `PROCESSING_MAX_PARALLELISM` | no | `1` | Maximum concurrent tile render jobs. |
| `MAX_CLOUD_FRACTION` | no | product default | Quality rejection threshold. |
| `TILE_MIN_ZOOM` | no | service default | Minimum generated zoom. |
| `TILE_MAX_NATIVE_ZOOM` | no | service default | Maximum generated native zoom. |
| `TILE_MAX_DISPLAY_ZOOM` | no | service default | Maximum display zoom advertised in manifests. |
| `TILE_SIZE` | no | `256` | Generated tile size in pixels. |
| `TILE_FORMAT` | no | `png` | Generated tile format. |
| `TILE_CLASSIFICATION_VERSION` | no | `radiance-dark-sky-v1` | Tile classification version. |
| `TILE_RENDER_VERSION` | no | `tiles-v1` | Tile renderer version. |
| `TILE_CDN_BASE_URL` | no | service default | Base URL used to build tile URL templates. |
| `TILE_BOUNDS` | no | service default | Generated tile bounds as `west,south,east,north`. |
| `TILE_IMMUTABLE_CACHE_CONTROL` | no | immutable cache header | Cache header for immutable tile and manifest blobs. |
| `TILE_LATEST_CACHE_CONTROL` | no | latest cache header | Cache header for `manifests/latest.json` and latest manifest API responses. |
| `RAW_GRANULE_RETENTION_DAYS` | no | service default | Raw artifact retention window. |
| `PROCESSED_TILE_SET_RETENTION_DAYS` | no | service default | Tile-set retention window. |
| `RETENTION_PROTECTED_PRIOR_TILE_SETS` | no | `2` | Prior non-latest tile sets protected per classification version. |
| `RETENTION_BATCH_LIMIT` | no | service default | Candidate selection cap per cleanup run. |
| `RETENTION_TILE_BLOB_LIMIT` | no | `5000` | Max listed blobs under one tile-set prefix before skip. |

## API Gateway Variables

| Variable | Required | Default | Purpose |
| --- | --- | --- | --- |
| `API_GATEWAY_PORT` | no | `8080` via helper | Local listener port. |
| `PORT` | no | `8080` inside service | Service listener port. |
| `RUNTIME_ENVIRONMENT` | no | `local` | Runtime profile for local/dev/staging/prod feature gates. |
| `JWT_ISSUER` | yes | local placeholder in helper | Admin JWT issuer. |
| `JWT_AUDIENCE` | yes | local placeholder in helper | Admin JWT audience. |
| `JWKS_URL` | yes | local placeholder in helper | JWKS endpoint. |
| `JWT_TENANT_ID` | required for staging/prod profile | unset | Concrete tenant id for tenant-bound auth. |
| `ADMIN_ROLE_CLAIM` | no | `roles` | Claim containing admin role values. |
| `ADMIN_REQUIRED_ROLE` | no | `lumenhorizon.admin` | Required admin role. |
| `RATE_LIMIT_BACKEND` | no | `memory` | `memory` or `redis`. |
| `REDIS_URL` | required when backend is `redis` | unset | Redis-compatible rate-limit store URL. |
| `DATABASE_MAX_CONNECTIONS` | no | `5` | Gateway database pool size. |
| `INGEST_SERVICE_BASE_URL` | no | unset | Ingest admin upstream URL. |
| `INTERNAL_SERVICE_AUTH_TOKEN` | no | unset | Gateway-to-ingest admin token. |
| `TILE_LATEST_CACHE_CONTROL` | no | `public, max-age=300, must-revalidate` | Latest manifest cache header. |
| `MAX_URL_LENGTH_BYTES` | no | `8192` | Request URL length limit. |
| `ADMIN_MAX_BODY_BYTES` | no | `65536` | Admin write body size limit. |
| `PUBLIC_ROUTE_TIMEOUT_SECONDS` | no | `5` | Public route timeout. |
| `ADMIN_ROUTE_TIMEOUT_SECONDS` | no | `15` | Admin route timeout. |
| `HEALTH_ROUTE_TIMEOUT_SECONDS` | no | `2` | Health/readiness route timeout. |

## Synchronization Checklist

When a service config key changes, update these files in the same change:

- [.env.example](../../../.env.example)
- [docs/DEVELOPER_GUIDE.md](../../../docs/DEVELOPER_GUIDE.md)
- this reference file

For API-visible config such as route limits, cache headers, or auth posture, also check [docs/API_GUIDE.md](../../../docs/API_GUIDE.md) and [../70-public-api-and-clients.md](../70-public-api-and-clients.md).

## Secret Handling

- Do not commit real bearer tokens, database passwords, storage keys, Redis URLs, or JWT signing material.
- Error messages must not echo secret values.
- Tests should use obvious fixture strings rather than secret-shaped values.
