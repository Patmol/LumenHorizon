# Investigation: Local Tile Serving For The Native App

## Status

Implemented with Option A. Local setup now makes the `processed-tiles` Azurite
container anonymously blob-readable and stamps newly generated manifests with
the local Azurite blob URL.

## Decision

Chose **Option A - Serve Azurite tiles anonymously and set a local CDN base**.
Rendered tile upload keys already match
`tiles/{tile_set_id}/{z}/{x}/{y}.png`, so adding the container-qualified
Azurite base URL preserves the production-style direct tile-loading contract
without adding a dev-only gateway tile server. Production behavior remains
unchanged because `processing-svc` still defaults to the production CDN when
`TILE_CDN_BASE_URL` is unset.

## Owner Handoff

This document is a self-contained task brief. You can act on it without prior
context from the app work that surfaced it. Read **Problem**, confirm the
**Root Cause** against the current code, then implement one of the
**Candidate Fixes** and meet the **Acceptance Criteria**.

## Severity

High for local developer experience. It blocks the end-to-end "see real tiles
in the app" smoke. It does not affect production correctness or the deployed
CDN path.

## Problem

When the native app (iOS Simulator / macOS) runs against a local backend, the
map overlay never shows tile imagery. The backend is healthy (the manifest and
tile-set endpoints return data), and the app correctly requests the right
tiles, but every tile request fails DNS resolution because the manifest
advertises the **production CDN host**.

The app loads tiles directly from the manifest `tile_url_template` (per the API
contract), so it faithfully uses whatever host the backend stamps into the
manifest.

## Evidence

App-side tile fetch logs (one per tile, over the continental US at z10):

```text
<DarkSkyTileOverlay>: Error loading URL
https://tiles.lumenhorizon.com/tiles/2026-06-07-radiance-dark-sky-v1-2517ab02-a1/10/163/385.png:
Error Domain=NSURLErrorDomain Code=-1003
"A server with the specified hostname could not be found."
```

- `Code=-1003` is DNS resolution failure: `tiles.lumenhorizon.com` does not
  resolve from a local machine.
- The requested `z/x/y` values (z10, x163, y385+) are correct: within the
  tile-set `min_zoom`/`max_native_zoom` and bounds. The app overlay, zoom, and
  bounds logic are working as intended.

## Root Cause

The processing service builds `tile_url_template` from a configurable CDN base
URL that **defaults to the production CDN** and has **no local override**.

- `backend/processing-svc/src/config.rs`
  - `const DEFAULT_TILE_CDN_BASE_URL: &str = "https://tiles.lumenhorizon.com";`
  - The value is read from the `TILE_CDN_BASE_URL` environment variable
    (falling back to the default) when building `AppConfig`.
- `backend/processing-svc/src/manifest.rs`
  - `build_tile_url_template(cdn_base_url, tile_set_id)` returns
    `"{cdn_base_url}/tiles/{tile_set_id}/{z}/{x}/{y}.png"`.

Historically, local setup copied or fell back to the production host, so locally
generated manifests embedded `https://tiles.lumenhorizon.com`. The local fix is
documented in `.env.example`; existing `.env` files must be updated before
regenerating manifests.

### Why the gateway redirect route does not help

The API Gateway exposes `GET /api/v1/tiles/{tile_set_id}/{z}/{x}/{y}.png`, but
it is a **302 redirect to the manifest `tile_url_template`**, not a tile
server. See `backend/api-gateway/src/server/routes.rs` (the tile redirect
handler): it loads the manifest, validates zoom/bounds, substitutes
`{z}/{x}/{y}` into `tile_url_template`, and returns `302 Found` with that URL in
`Location`. Pointing the app at the gateway route therefore bounces straight
back to `tiles.lumenhorizon.com` and fails identically.

### Where local tiles actually live

Rendered tiles are uploaded to local **Azurite** blob storage (the Azure Blob
emulator) defined in `compose.yml` (`azurite`, blob port `10000`). The
processing service uploads processed tiles to the container named by
`DEFAULT_PROCESSED_TILES_CONTAINER` (`"processed-tiles"`) in
`backend/processing-svc/src/config.rs`. Local setup now grants anonymous blob
read access for that container, so manifest templates can use
`http://127.0.0.1:10000/devstoreaccount1/processed-tiles/tiles/{tile_set_id}/{z}/{x}/{y}.png`.

## Reproduction

1. `just up && just migrate && just serve-api`
2. Ensure a tile set has been processed so a latest manifest exists.
3. Inspect the advertised template host:
   ```bash
   curl -s http://127.0.0.1:8080/api/v1/tiles/manifest \
     | jq '.data.tile_set_id, .data.tile_url_template'
   ```
   Observe the host is `https://tiles.lumenhorizon.com`.
4. Fetch a single tile and observe it does not resolve locally:
   ```bash
   curl -sI "https://tiles.lumenhorizon.com/tiles/<tile_set_id>/10/163/385.png"
   ```

## Goal

A local developer can run the backend, run the app pointed at the local API,
and see real dark-sky tile imagery over the covered region, without manual
per-run rewriting of manifests.

## Candidate Fixes

Pick one; record the decision and rationale.

### Option A - Serve Azurite tiles anonymously and set a local CDN base

1. Configure the `processed-tiles` container (or a dedicated public tiles
   container) for anonymous blob read in the local Azurite setup.
2. Ensure the uploaded blob key layout matches
   `tiles/{tile_set_id}/{z}/{x}/{y}.png` (verify the exact upload key the
   processing service writes; adjust the template base or the upload path so
   they line up).
3. Set `TILE_CDN_BASE_URL` for local `processing-svc` to the Azurite blob URL
   (e.g. `http://127.0.0.1:10000/devstoreaccount1/<container>`), wired in
   `compose.yml` and/or the dev scripts.
- Tradeoff: closest to the real CDN flow; requires getting the Azurite
  container path and public-access details exactly right.

### Option B - Add a local static tile-serving endpoint

1. Add a small local service (or a gateway dev-only route) that streams the
   tile blob bytes from Azurite at
   `/tiles/{tile_set_id}/{z}/{x}/{y}.png` (server-side fetch with credentials,
   anonymous response to the client).
2. Set `TILE_CDN_BASE_URL` to that local endpoint's base.
- Tradeoff: gives a clean local URL and keeps storage credentials server-side,
  but adds a dev-only serving path to maintain. If added to the gateway, keep
  it clearly dev-only and out of the production contract.

### Common to either option

- Changing `TILE_CDN_BASE_URL` only affects **newly generated** manifests.
  Regenerate the tile set (re-run processing) so the latest manifest carries
  the new template; existing manifests in the DB keep the old host.
- Keep cleartext-HTTP local URLs compatible with the app's ATS local-networking
  exception (loopback hosts are already permitted).

## Acceptance Criteria

1. After `just up && just migrate && just serve-api` and processing a tile set,
   `curl …/api/v1/tiles/manifest | jq '.data.tile_url_template'` returns a host
   reachable from the local machine.
2. `curl -s "<substituted tile URL>" -o tile.png` returns a valid PNG
   (`file tile.png` reports PNG image data) without auth headers.
3. The native app, pointed at the local API, renders tile imagery over the
   covered region at zoom >= `min_zoom`.
4. The local configuration is documented (`compose.yml`/dev scripts/`.env`
   example) so a clean clone works without manual manifest edits.
5. The production manifest path is unchanged: with `TILE_CDN_BASE_URL` unset,
   manifests still use the production CDN host.

## Out Of Scope

- Native app code changes. The app overlay/zoom/bounds logic is verified
  correct; do not modify the app to work around the backend host. A deliberate
  gateway-mediated/dev tile-loading mode in the app is explicitly deferred by
  the app API contract and is not part of this fix.
- Production CDN deployment or DNS for `tiles.lumenhorizon.com`.

## References

- `backend/processing-svc/src/config.rs` - `DEFAULT_TILE_CDN_BASE_URL`,
  `TILE_CDN_BASE_URL`, `DEFAULT_PROCESSED_TILES_CONTAINER`.
- `backend/processing-svc/src/manifest.rs` - `build_tile_url_template`.
- `backend/api-gateway/src/server/routes.rs` - tile redirect handler (302 to
  `tile_url_template`).
- `compose.yml` - `azurite` blob emulator (port `10000`).
- `plans/app/reference/api-and-data-contracts.md` - app tile-loading contract
  (prefers manifest template; redirect route is deferred).
- `plans/app/status/gap-register.md` - app gap `APP-003` references this
  investigation.
