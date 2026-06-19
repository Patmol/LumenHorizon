# 50. Ingestion Service

`ingest-svc` is responsible for discovering source granules, downloading raw data, storing raw artifacts, and emitting processing work.

## Commands

| Command | Purpose |
| --- | --- |
| `serve` | Start the local HTTP service. |
| `ingest` | Run one ingestion pass for the selected cadence. |
| `recover-ingest` | Recover downloaded/validated rows and pending enqueue outbox records. |
| `replay-rejected <ingest-id>` | Move one rejected row back through a deliberate replay path. |

## HTTP Routes

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/health` | Dependency-free liveness. |
| `GET` | `/ready` | PostgreSQL, raw blob storage, and queue readiness. |
| `POST` | `/admin/ingest/trigger` | Local/internal trigger route. |

## Ingest Flow

1. Select products by cadence.
2. Query CMR with paging and resume constraints.
3. Download raw granules from Earthdata.
4. Validate basic HDF5/HDF-EOS5 compatibility.
5. Store raw bytes under a relative blob path.
6. Persist ingest metadata and status transitions.
7. Create durable enqueue outbox records.
8. Emit processing queue messages.

## Reliability Rules

- Duplicate logical ingest rows must not create duplicate durable work.
- Rejected rows must not advance discovery resume points.
- Queue emission is at least once; processing idempotency is required downstream.
- Error messages should be contextual without exposing secrets.
- Storage paths persisted in the database are relative paths, not service URLs.

## Verification

- Unit tests for CMR parsing, pagination, storage path validation, config errors, and queue message shape.
- SQLx tests for duplicate rows, status transitions, recovery outbox behavior, and replay state.
- Local smoke with `just ingest daily` or `just ingest monthly` when credentials and local dependencies are available.
