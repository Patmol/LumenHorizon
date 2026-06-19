# 01. Professional Grade Checklist

This checklist defines the quality bar for the active local/backend project.

## Product And Data

- Product claims are conservative and traceable to VIIRS radiance data.
- Dataset mappings are verified and documented.
- Quality/cloud filtering is explicit and tested.
- Public wording avoids measured Bortle, SQM, certified observing quality, or site-score claims unless a calibrated future product exists.

## Backend Services

- Services have clear command boundaries.
- Configuration validation is fail-fast and does not echo secrets.
- Database writes are idempotent where retries can happen.
- Blob and queue paths are relative and stable.
- Health is dependency-free; readiness reports dependency state.

## API

- `/api/v1` contracts are represented in OpenAPI.
- Responses use stable envelopes and request IDs.
- Admin routes require authentication, authorization, and audit logging.
- Rate-limit behavior is tested for route classes.
- Errors are sanitized and avoid internal details.

## Local Development And CI

- `just setup`, `just up`, `just migrate`, `just serve`, and `just serve-api` work from a clean clone.
- `just validate` runs local contract, Rust, test, lint, and Docker checks.
- CI mirrors local checks and scans built images.
- CI does not require remote credentials.

## Operations Docs

- Status docs describe current local/backend reality.
- Runbooks cover local restore practice, queue backlog investigation, and retention cleanup.
- Configuration reference matches `.env.example` and service config structs.
- Deleted or out-of-scope hosting automation is not referenced from active docs.
