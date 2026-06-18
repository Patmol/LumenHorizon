# Observing Sites And Sky-Quality Scores

Status: Future feature plan. Deferred so the active API work can focus on tile manifests and `MKTileOverlay` compatibility.

## Product Intent

Help users find practical public observing locations and understand nearby sky quality without requiring an account. The feature should support anonymous read-only client workflows while preserving the API Gateway security posture from Chunk 9.

## Proposed Scope

- Implement public observing site lookup through `api-gateway`.
- Provide public site detail responses for approved, system-curated, or imported public datasets.
- Implement sky-quality score responses for a site using the latest tile manifest, nearest dark-sky classification, data freshness, and explicitly modeled future signals such as weather or cloud cover.
- Define the data ownership boundary for `sites-svc` or an equivalent service/module before implementation.
- Add database tables, migrations, indexing, and import/update workflows for approved site data.
- Add OpenAPI contract coverage for route shapes, query parameters, response envelopes, error codes, auth posture, and rate-limit behavior.
- Integrate the Apple client with site lookup and score reads after backend contracts are stable.

## Candidate Public Routes

These are placeholders for discussion, not active implementation commitments.

| Method | Path | Auth | Purpose |
|--------|------|------|---------|
| `GET` | `/api/v1/sites` | anonymous | Search public observing sites near coordinates or inside bounds. |
| `GET` | `/api/v1/sites/{site_id}` | anonymous | Return public site detail. |
| `GET` | `/api/v1/sites/{site_id}/score` | anonymous | Return current sky-quality score for the site. |

Candidate search parameters:

| Parameter | Required | Notes |
|-----------|----------|-------|
| `lat` | required with `lon` | Center latitude. |
| `lon` | required with `lat` | Center longitude. |
| `radius_km` | no | Proposed default `50`, proposed max `250`. |
| `west`, `south`, `east`, `north` | alternative | Bounding-box search. |
| `limit` | no | Proposed default `50`, proposed max `100`. |
| `cursor` | no | Opaque pagination cursor. |

## Score Semantics To Decide

- Whether the public score is a single integer `0..100`, a class label, or both.
- How to combine dark-sky class, tile data freshness, moon phase, cloud forecast, weather, and site metadata.
- How much explanatory metadata the client needs when data is stale, missing, or forecast signals are unavailable.
- Whether score responses are computed on demand, cached per site/tile set, or materialized during tile publication.
- How to avoid implying unsafe precision from VIIRS resolution, forecast uncertainty, or stale composite data.

## Privacy And Safety Notes

- Do not log precise user locations in application logs, audit events, metrics, or rate-limit keys unless a privacy review explicitly approves the field and retention policy.
- First implementation should prefer public, curated site data over user-submitted private locations.
- Anonymous routes must remain read-only and rate-limited.
- Admin or import workflows for site data must remain protected and audited.

## Dependencies

- Chunk 9 API Gateway foundation, including auth, rate limits, request IDs, sanitized errors, and OpenAPI validation.
- Chunk 9 tile manifest endpoints and client tile overlay integration.
- Tile generation and latest manifest publication from Chunk 8.
- A decision on whether `sites-svc` is a separate service, an `api-gateway` module, or a later split after the data model stabilizes.
- Approved source datasets and licensing/attribution requirements for public observing sites.

## Out Of Scope Until Promoted

- User accounts, private saved locations, reviews, comments, or community submissions.
- Production browser CORS changes unless a browser client is explicitly added.
- Admin self-service tooling beyond protected import/update operations needed for site data.
- Weather/cloud provider integration unless the score design explicitly includes it and operational dependencies are reviewed.

## Open Questions And Comments

- Which public observing site datasets are acceptable for the first implementation?
- Is PostGIS required from the start, or are latitude/longitude indexes sufficient for the initial dataset size?
- Should scores be available only for curated sites, or also for arbitrary coordinates later?
- What attribution must be shown in the Apple client for site data and derived sky-quality data?
- What stale-data threshold should make the score unavailable instead of merely degraded?

## Promotion Checklist

- Product behavior and client screens are clear enough to test.
- Public route contracts are reviewed and added to `backend/api-gateway/openapi/openapi.yaml`.
- Security and identity docs explicitly include the promoted site/score route classes.
- Database schema and migration ownership are finalized.
- Rate limits, cache headers, privacy rules, and logging fields are documented.
- Tests cover route contracts, coordinate validation, pagination, rate limiting, read-only anonymous access, score semantics, stale-data behavior, and admin/import protection.