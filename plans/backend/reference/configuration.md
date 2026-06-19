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
| `PROCESSING_MAX_MESSAGES` | no | `1` | Worker batch size. |
| `PROCESSING_VISIBILITY_TIMEOUT_SECONDS` | no | `300` | Queue invisibility window during processing. |
| `PROCESSING_MAX_DEQUEUE_COUNT` | no | `5` | Dead-letter threshold. |
| `MAX_CLOUD_FRACTION` | no | product default | Quality rejection threshold. |
| `TILE_MIN_ZOOM` | no | service default | Minimum generated zoom. |
| `TILE_MAX_ZOOM` | no | service default | Maximum generated zoom. |
| `RETENTION_RAW_DAYS` | no | service default | Raw artifact retention window. |
| `RETENTION_TILE_SET_DAYS` | no | service default | Tile-set retention window. |
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
| `INGEST_SERVICE_BASE_URL` | no | unset | Ingest admin upstream URL. |
| `INTERNAL_SERVICE_AUTH_TOKEN` | no | unset | Gateway-to-ingest admin token. |
| `TILE_LATEST_CACHE_CONTROL` | no | `public, max-age=300, must-revalidate` | Latest manifest cache header. |

## Secret Handling

- Do not commit real bearer tokens, database passwords, storage keys, Redis URLs, or JWT signing material.
- Error messages must not echo secret values.
- Tests should use obvious fixture strings rather than secret-shaped values.
