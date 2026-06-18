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

| ID | Severity | Area | Gap | Target |
| --- | --- | --- | --- | --- |
| GAP-001 | High | Science fixtures | Keep representative VIIRS fixture coverage current for dataset mapping, quality filtering, rejection, and classification behavior. | Chunk 7 |
| GAP-002 | High | Tile validation | Add broader tile-generation smoke coverage for multiple products, bounds, and zoom ranges. | Chunk 8 |
| GAP-003 | Medium | Retention evidence | Capture local dry-run and execute evidence for raw and tile retention cleanup against seeded data. | Chunk 8 |
| GAP-004 | Medium | API clients | Add client-style smoke examples for manifest fetch, tile-set pagination, and tile redirect URL substitution. | Chunk 9 |
| GAP-005 | Medium | Operations | Expand local runbooks with concrete diagnostic commands and expected outputs. | Chunk 10 |
| GAP-006 | Low | Documentation | Keep docs and `.env.example` synchronized as service config changes. | Ongoing |

## Closed Local Gaps

| ID | Area | Resolution |
| --- | --- | --- |
| CLOSED-001 | Ingest idempotency | Duplicate logical ingest rows are covered by tests and status transitions. |
| CLOSED-002 | Processing idempotency | Processing uses `ingest_id` idempotency and dead-letter handling. |
| CLOSED-003 | Tile manifests | Processing publishes immutable manifests and a latest pointer. |
| CLOSED-004 | API contract | `just openapi-check` validates implemented gateway routes against OpenAPI. |
| CLOSED-005 | Local CI | `just validate` and CI cover Rust checks, tests, Docker builds, image scans, and API contract validation without remote credentials. |
