# Gap Register

This register tracks active local/backend product gaps only.

## Severity

| Severity | Meaning |
| --- | --- |
| Critical | Blocks core local product correctness or can corrupt data. |
| High | Blocks reliable local validation or API contract confidence. |
| Medium | Important hardening or evidence gap. |
| Low | Documentation or polish gap. |

## Active Gaps

No active local/backend product gaps remain in this register.

## Closed Local Gaps

| ID | Area | Resolution |
| --- | --- | --- |
| CLOSED-001 | Ingest idempotency | Duplicate logical ingest rows are covered by tests and status transitions. |
| CLOSED-002 | Processing idempotency | Processing uses `ingest_id` idempotency and dead-letter handling. |
| CLOSED-003 | Tile manifests | Processing publishes immutable manifests and a latest pointer. |
| CLOSED-004 | API contract | `just openapi-check` validates implemented gateway routes against OpenAPI. |
| CLOSED-005 | Local CI | `just validate` and CI cover Rust checks, tests, Docker builds, image scans, and API contract validation without remote credentials. |
| CLOSED-006 | Operations runbooks | Local PostgreSQL restore practice, queue backlog investigation, and retention cleanup runbooks live under `plans/backend/runbooks` with concrete diagnostic commands and expected outputs. |
| CLOSED-007 | Science fixtures | Representative daily/monthly VIIRS fixture tests cover dataset mapping, quality filtering, cloud rejection, observation-count evidence, and dark-sky classification behavior. |
| CLOSED-008 | Tile validation | Processing tests cover multi-product tile-generation smoke matrices across products, bounds, zoom ranges, manifest counts, and latest pointer consistency. |
| CLOSED-009 | Retention evidence | `just retention-cleanup` and `just retention-cleanup-execute` were verified against seeded local data during gap closure; tests cover cleanup selection and safety rules. |
| CLOSED-010 | API client examples | API docs and gateway tests cover latest manifest fetch, tile-set pagination, and tile redirect URL substitution. |
| CLOSED-011 | Documentation sync | `.env.example`, developer docs, API docs, configuration reference, and verification docs are synchronized with current service config and smoke command surfaces. |
