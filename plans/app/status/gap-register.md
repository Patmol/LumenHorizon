# App Gap Register

This register tracks active native app product gaps.

## Severity

| Severity | Meaning |
| --- | --- |
| Critical | Blocks core app correctness or can misrepresent backend data. |
| High | Blocks reliable app validation or core map functionality. |
| Medium | Important hardening, usability, or evidence gap. |
| Low | Documentation or polish gap. |

## Active Gaps

| ID | Severity | Area | Gap | Target chunk |
| --- | --- | --- | --- | --- |
| APP-001 | High | Project foundation | No iOS, macOS, and visionOS project targets, shared module structure, or app configuration exists in the app plan baseline. | Chunk 1 |
| APP-002 | Critical | Backend contracts | No typed Swift client, envelope decoding, manifest validation, or class metadata model exists. | Chunk 2 |
| APP-003 | Critical | Map rendering | No MapKit tile overlay integration exists for backend PNG tiles. | Chunk 3 |
| APP-004 | High | Product copy | No app UX exists to explain VIIRS radiance evidence, transparent pixels, limitations, or non-goals. | Chunk 4 |
| APP-005 | Medium | Tile-set selection | No historical/latest tile-set browser or selected tile-set persistence exists. | Chunk 5 |
| APP-006 | Medium | Offline resilience | No metadata cache, freshness labels, or offline fallback exists. | Chunk 6 |
| APP-007 | Medium | Platform polish | iOS, iPadOS, macOS, and visionOS navigation, commands, layout, and accessibility are undefined. | Chunk 7 |
| APP-008 | High | Validation | No app build/test commands, fixtures, UI smoke tests, or CI jobs exist. | Chunk 8 |
| APP-009 | Low | Launch readiness | No app developer guide, privacy note, or local backend smoke guide exists. | Chunk 9 |

## Closed Gaps

| ID | Area | Resolution |
| --- | --- | --- |
| APP-001 | Project foundation | Multiplatform Xcode project (iOS/macOS/visionOS) builds in Debug and Release; the shared `AppCore` Swift package provides app configuration, environment-driven API base URLs, and map defaults; environment values flow from Debug/Release build settings through `Info.plist` into `AppRuntimeConfiguration`; `AppCore` unit tests cover configuration parsing and the release-points-at-local guard. |
