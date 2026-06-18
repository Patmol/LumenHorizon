# 10. Product And Architecture

## Product Goal

LumenHorizon helps users understand broad dark-sky conditions from VIIRS Black Marble data. The backend ingests source products, processes quality-filtered radiance evidence, generates map tiles, and exposes product metadata through a local API Gateway.

## Non-Goals

- Do not claim measured Bortle class, SQM readings, certified observing quality, or observing-site scores from the current radiance product.
- Do not make remote hosting or release automation part of the active architecture.
- Do not replace the current Azurite-compatible storage runtime without a separate storage design.

## Backend Components

| Component | Responsibility |
| --- | --- |
| `ingest-svc` | CMR discovery, Earthdata download, raw blob persistence, processing queue emission, recovery, and replay. |
| `processing-svc` | Queue consumption, science processing, quality filtering, classification evidence, tile generation, manifests, and retention cleanup. |
| `api-gateway` | Product/admin HTTP API, request hardening, auth, rate limiting, tile metadata, and admin forwarding. |
| `db-migrate` | SQLx migration runner. |
| `shared` | Narrow contracts and reusable helpers shared across services. |

## Local Runtime

```text
client or developer tool
  -> api-gateway for /api/v1 routes
  -> ingest-svc for local ingest development routes
  -> PostgreSQL for metadata
  -> Azurite-compatible blob and queue storage
```

PostgreSQL stores ingest rows, processing logs, tile-set metadata, recovery outbox records, and retention cleanup audit records. Blob storage holds raw granules and processed tile artifacts. Queue storage holds processing work messages and dead-letter messages.

## Data Flow

1. `ingest-svc` discovers granules from CMR for configured products and cadence.
2. Ingest downloads raw granules and writes them to raw blob storage.
3. Ingest persists metadata and emits processing queue messages.
4. `processing-svc` receives messages and creates idempotent processing log entries.
5. Processing reads source datasets through the GDAL boundary, applies quality filtering, records summary evidence, and classifies accepted granules.
6. Accepted processing messages generate Slippy Map tile sets, immutable manifests, a latest pointer, and `tile_sets` metadata.
7. `api-gateway` exposes product metadata, tile manifests, tile redirects, admin run lists, ingest trigger, and processing requeue.
8. Retention cleanup protects latest plus prior tile sets and removes eligible stale artifacts only in explicit execute mode.

## Design Rules

- Use structured parsers and typed models for data contracts.
- Keep service-owned workflow logic out of `shared`.
- Keep low-level shared protocol helpers in `shared` when multiple services use them.
- Prefer local deterministic validation over manual inspection.
- Treat storage backend replacement as future architecture work, not an incidental rename.
