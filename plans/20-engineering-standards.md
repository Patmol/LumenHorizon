# 20. Engineering Standards

## Rust

- Keep crates small and service-owned behavior in the owning service crate.
- Use `thiserror` for domain and service errors.
- Avoid panics in request, ingest, processing, and cleanup paths.
- Validate configuration at startup or command entry.
- Do not echo secret values in errors or logs.
- Keep async boundaries explicit and testable.

## Shared Crate

Use `backend/shared` only for low-level behavior shared by more than one crate:

- stable DTOs and message contracts
- product/cadence metadata
- PostgreSQL pool construction
- tracing initialization
- HTTP retry policy
- Slippy Map tile math
- Blob/Queue REST endpoint construction and signing helpers
- queue/blob name validation and protocol body formatting

Do not put service workflow policy in `shared`. Queue loops, retries, dead-letter decisions, database writes, API handlers, and processing workflows remain service-owned.

## Database

- Schema changes belong in `backend/db-migrate/migrations`.
- Migrations should be deterministic, idempotent where possible, and reviewed with affected service code.
- Services should not apply migrations during normal startup.
- Database writes that can be retried must have idempotency keys or unique constraints.

## Ingest And Processing

- Persist relative blob paths rather than full service URLs.
- Treat queue delivery as at least once.
- Delete queue messages only after durable success.
- Record terminal failures with enough context to diagnose without exposing secrets.
- Keep product claim language tied to the evidence actually computed.

## API

- Keep `/api/v1` backward compatible except for additive changes.
- Use stable response envelopes.
- Include request IDs.
- Require admin auth and route-to-role policy coverage for admin routes.
- Keep CORS disabled unless an explicit browser client is added.
- Keep OpenAPI in sync with implemented routes.

## Tests

- Add focused unit tests for parsing, validation, and pure logic.
- Add SQLx tests for schema-backed behavior and idempotency.
- Add route tests for API envelopes, auth, rate limits, and hardening.
- Keep fixtures obviously non-secret.
- Run the narrowest relevant check while developing, then broader checks before closing shared or cross-service changes.
