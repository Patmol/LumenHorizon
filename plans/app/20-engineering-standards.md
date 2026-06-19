# 20. Engineering Standards

## Swift And SwiftUI

- Prefer Swift concurrency (`async`/`await`) for backend calls and cache access.
- Keep UI state updates on the main actor.
- Use typed errors for API, decoding, validation, cache, and network failures.
- Avoid force unwraps in app, networking, persistence, and map-overlay paths.
- Keep SwiftUI views small; move data loading and state transitions into testable models.
- Keep preview data in explicit fixture builders and avoid hidden network calls in previews.

## MapKit

- Isolate `MKMapView` and `MKTileOverlay` integration behind adapter types.
- Recreate tile overlays when `tile_set_id` or URL template changes.
- Validate tile URL templates before constructing overlays.
- Use manifest bounds and zoom metadata to drive camera constraints, empty states, and explanatory copy.
- Do not recolor or reinterpret PNG tile pixels in the client.

## Backend API

- Decode the backend `ApiEnvelope` before decoding route-specific `data`.
- Preserve backend `request_id` in diagnostics and user-facing support details.
- Treat unknown additive fields as compatible.
- Treat missing required manifest fields, invalid bounds, invalid zoom ranges, or missing URL placeholders as data-unavailable errors.
- Never infer tile storage paths; use `tile_url_template` or documented redirect routes.
- Keep retry behavior limited and visible. Do not spin or silently suppress backend errors.

## Persistence

- Persist only small metadata and user preferences by default.
- Let URL loading cache handle PNG tile caching unless a separate offline-map plan is approved.
- Store enough freshness metadata to label cached content accurately.
- Do not mark cached data as latest unless it was refreshed successfully.

## Privacy And Permissions

- Location access is optional and used only to center the map when the user chooses it.
- The app must remain useful when location permission is denied.
- Do not collect precise location history in the first app version.
- Do not add analytics, crash reporting, or remote logging without a separate privacy review.

## Accessibility

- Provide VoiceOver labels for legend classes, data freshness, overlay opacity, and map actions.
- Ensure legend colors are accompanied by labels and radiance ranges.
- Support Dynamic Type in controls, panels, and explanatory copy.
- Maintain keyboard navigation for macOS, iPad hardware keyboards, and visionOS text/input workflows where available.

## Tests

- Add focused unit tests for DTO decoding, URL-template validation, map state transitions, cache behavior, and backend error mapping.
- Add integration tests with `URLProtocol` or equivalent request stubs rather than live network dependencies.
- Add UI smoke tests for launch, map loading states, legend, tile-set picker, and platform navigation.
- Keep fixtures obviously non-secret and synchronized with backend examples.
