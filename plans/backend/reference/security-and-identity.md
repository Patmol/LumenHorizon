# Security And Identity

## Local Secret Rules

- Keep real secrets out of source control.
- Use `.env` or protected local environment variables for developer secrets.
- Keep `.env.example` populated only with safe local defaults and placeholders.
- Configuration errors must name missing variables without echoing secret values.
- Tests should use obvious fixture strings.

## Admin Authentication

`api-gateway` validates admin JWTs using configured issuer, audience, JWKS URL, optional tenant id, role claim, and required role. The first-version admin role remains `lumenhorizon.admin` unless config overrides it.

Admin routes must keep these properties:

- Authentication before handler execution.
- Route-to-role policy coverage in tests.
- Audit events for admin reads and writes.
- No request body, token, secret, or upstream response body logging.
- Sanitized auth and authorization errors.

## Internal Admin Calls

Gateway-to-ingest admin calls use `INTERNAL_SERVICE_AUTH_TOKEN` and `INTERNAL_SERVICE_AUTH_HEADER` when configured. `ingest-svc` enforces the same token on admin routes when the token is present.

## API Hardening

- Gateway responses include request IDs.
- Security headers are applied to gateway responses.
- CORS is disabled by default.
- URL length, request body size, method, path, and query validation are explicit.
- Rate limits apply by route class.
- Error responses avoid stack traces, connection strings, tokens, keys, and raw upstream details.

## Storage Access

The active storage runtime uses account/key authentication for local Azurite-compatible Blob and Queue REST APIs. Storage keys must be treated as secrets even when local defaults use well-known emulator values.
