# API Gateway JWT Validation Libraries

Status: Future implementation note. Deferred from Chunk 9 unless the current local JWT/JWKS validation code becomes difficult to test, maintain, or harden.

## Product Intent

Keep the API Gateway admin-auth boundary secure and understandable while reducing custom security plumbing where a mature Rust library can safely own generic JWT or OpenID Connect mechanics.

The goal is not to outsource LumenHorizon authorization policy. The gateway must continue to own tenant boundaries, accepted audiences, admin role requirements, route protection, sanitized errors, and fail-closed behavior.

## Current Chunk 9 Position

The current gateway uses `jsonwebtoken` for JWT signature verification and implements the surrounding Microsoft Entra access-token policy locally in `backend/api-gateway/src/auth.rs` and `backend/api-gateway/src/config.rs`.

Current local responsibilities include:

- Extract `Authorization: Bearer ...` tokens.
- Decode JWT headers and require `RS256`.
- Fetch and cache JWKS keys.
- Select signing keys by `kid`.
- Verify signatures with `jsonwebtoken`.
- Validate issuer, audience, subject, expiry, not-before, issued-at, and maximum token lifetime.
- Bind staging/prod auth to a concrete Microsoft Entra tenant with `JWT_TENANT_ID` and the token `tid` claim.
- Reject `common`, `organizations`, and `consumers` issuer/JWKS URLs in staging/prod.
- Extract the configured admin role claim and require `lumenhorizon.admin` by default.
- Return sanitized API Gateway error envelopes without exposing raw tokens, keys, claims, or upstream errors.

## Candidate Libraries

These are candidates to evaluate, not implementation commitments.

| Library | Potential use | Notes |
|---------|---------------|-------|
| `jsonwebtoken` | Keep as low-level JWT verification primitive. | Already in use. Widely used and explicit, but JWKS fetching/cache and policy remain local. |
| `aliri` / `aliri_oauth2` | Replace more of the generic JWT/JWKS validation path. | Promising candidate for reducing hand-rolled JWKS validation. Must confirm Microsoft Entra access-token behavior, cache control, error handling, and Axum integration. |
| `openidconnect` | OpenID Connect discovery and token-related primitives. | Strong OpenID Connect library, but may be better suited to ID-token/client flows than API access-token authorization. LumenHorizon would still need local access-token, tenant, audience, and role policy. |
| Axum JWT middleware crates | Reduce route middleware boilerplate. | Use cautiously. Must support Entra access tokens, JWKS refresh, tenant binding, role claims, sanitized envelopes, and route-class-specific auth behavior. |

## Policy That Must Stay Local

Any future library adoption must preserve explicit local ownership of:

- Accepted Microsoft Entra tenant id and `tid` claim matching.
- Exact `JWT_ISSUER`, `JWT_AUDIENCE`, and `JWKS_URL` configuration policy.
- Rejection of multi-tenant placeholders in staging/prod.
- Admin role claim name and required role value.
- Route-level public/admin separation.
- Admin audit logging and redaction rules.
- Auth-failure rate limiting.
- Sanitized `401`, `403`, `429`, and `503` API envelopes.
- Startup/readiness behavior for missing or unsafe auth configuration.

## When To Promote This Work

Consider promoting this note into an implementation chunk if one or more of these becomes true:

- RS256/JWKS fixture tests reveal tricky edge cases in the current implementation.
- JWKS cache refresh, key rotation, retry, or error handling becomes complex enough to justify a higher-level library.
- The gateway needs OpenID Connect discovery metadata instead of explicit issuer/JWKS configuration.
- Multiple services need the same JWT validation mechanics and a shared internal verifier becomes valuable.
- Security review asks to reduce custom JWT validation code in favor of a focused, maintained crate.

Do not promote this work just to reduce line count. A small explicit policy layer is preferable to a generic abstraction that hides tenant, audience, and role decisions.

## Evaluation Checklist

Before replacing the current internals, verify the candidate library can support:

- Microsoft Entra v2 access tokens for a single-tenant API app.
- RS256-only validation and rejection of unsupported algorithms.
- JWKS key lookup by `kid` and safe behavior on unknown keys.
- JWKS cache expiry and refresh behavior compatible with gateway readiness and failure handling.
- Explicit issuer and audience validation.
- Explicit tenant-id claim validation or easy access to validated claims for local `tid` checks.
- Maximum token lifetime enforcement.
- Configurable clock skew.
- Clear distinction between unauthenticated and forbidden failures.
- Sanitized errors that do not expose token contents, JWKS URLs with secrets, raw upstream responses, or stack traces.
- Tests that can run without live Microsoft Entra calls.

## Promotion Checklist

- Current `auth.rs` responsibilities are split into generic verification and local policy before or during the change.
- Fixture-backed RS256/JWKS tests exist for valid token, wrong issuer, wrong audience, wrong tenant, missing tenant, expired token, future `nbf`, excessive lifetime, missing role, unsupported algorithm, missing `kid`, unknown `kid`, and malformed JWKS.
- Documentation still tells operators to use tenant-specific issuer/JWKS URLs and `JWT_TENANT_ID` in staging/prod.
- Gateway error envelopes and status codes remain stable for clients.
- Auth-failure rate limiting and admin audit behavior remain unchanged.
- Focused `api-gateway` tests and full backend compile checks pass.