# App Plans Overview

These plans describe a draft native Apple application roadmap for LumenHorizon. The app consumes the existing backend tile manifest, class metadata, tile-set list, and PNG tile URL contracts to render VIIRS radiance-based dark-sky information with MapKit on iOS, macOS, and visionOS.

## Reading Order

1. [00-implementation-roadmap.md](00-implementation-roadmap.md) - continuous app chunk roadmap.
2. [10-product-and-architecture.md](10-product-and-architecture.md) - product goals and native app architecture.
3. [20-engineering-standards.md](20-engineering-standards.md) - Swift, SwiftUI, MapKit, privacy, and accessibility standards.
4. [30-local-development-and-ci.md](30-local-development-and-ci.md) - local setup and validation expectations.
5. [50-map-and-data-experience.md](50-map-and-data-experience.md) - map, overlay, legend, and data-selection behavior.
6. [60-backend-integration.md](60-backend-integration.md) - API client and tile-loading integration.
7. [status/index.md](status/index.md) - current implementation status.
8. [status/gap-register.md](status/gap-register.md) - open product and engineering gaps.

## Reference Docs

- [reference/api-and-data-contracts.md](reference/api-and-data-contracts.md)
- [reference/testing-and-verification.md](reference/testing-and-verification.md)

## Backend Dependencies

The app plan depends on the backend contract documented in [../backend/70-public-api-and-clients.md](../backend/70-public-api-and-clients.md) and [../backend/reference/message-and-api-contracts.md](../backend/reference/message-and-api-contracts.md). The current client workflow is:

1. Fetch `GET /api/v1/tiles/manifest`.
2. Fetch `GET /api/v1/tiles/classes`.
3. Configure a MapKit tile overlay from the manifest `tile_url_template`.
4. Use `bounds`, `min_zoom`, `max_native_zoom`, and `max_display_zoom` to constrain requests and presentation.
