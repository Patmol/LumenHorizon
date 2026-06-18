# Observability And Operations

## Logging

Services emit structured logs to stdout/stderr. Logs should include service name, version where available, command or request context, request IDs, correlation IDs, ingest IDs, processing IDs, and retry attempt fields where useful.

Do not log:

- bearer tokens
- storage keys
- database URLs with credentials
- Redis URLs with credentials
- JWTs or JWKS payloads beyond safe metadata
- request bodies for admin writes

## Health And Readiness

- `/health` is dependency-free liveness.
- `/ready` checks configured dependencies and reports sanitized component status.
- API Gateway readiness can run in local mode with optional backed dependencies absent.

## Local Operational Checks

- Use `just up` to start dependencies.
- Use `just migrate` to apply schemas.
- Use `just serve` and `just serve-api` for HTTP services.
- Use `just ingest daily` or `just ingest monthly` for controlled ingest.
- Use `just retention-cleanup` for dry-run cleanup review.
- Use `just validate` before closing broad backend changes.

## Alerts And Evidence

Active repository checks are local and CI-oriented. Operational evidence should be captured as command output, logs, database rows, queue state, and blob listings when validating ingest, processing, tile publication, or cleanup behavior.

## Runbooks

Current runbooks live under [../runbooks](../runbooks):

- database backup/restore practice
- queue backlog investigation
- retention cleanup

Runbooks should stay executable against local PostgreSQL and Azurite-compatible storage.
