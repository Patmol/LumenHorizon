# App Status Overview

## Current Snapshot

The app plan is a draft. No native iOS, macOS, and visionOS app implementation is currently represented in this plan set. The backend already exposes the anonymous tile manifest, tile classes, tile-set list, and PNG tile redirect/template contracts needed for a MapKit client.

## Already Done

- App planning structure exists under `plans/app`.
- Backend client contracts are identified from existing backend docs and code.
- Initial chunk roadmap, architecture, standards, integration, testing, and gap docs are drafted.

## Not Started

- Xcode project/workspace.
- Shared Swift app modules.
- Backend API client.
- MapKit tile overlay.
- Legend and dataset metadata UI.
- Tile-set selection.
- App tests and CI.
- App developer guide.

## Current Roadmap Position

The active app roadmap starts at Chunk 0 in [../00-implementation-roadmap.md](../00-implementation-roadmap.md). Chunks 0-2 establish architecture, project foundation, and backend client contracts. Chunks 3-6 build the map experience, legend, tile-set selection, and cache resilience. Chunks 7-9 cover native polish, quality gates, and local launch readiness.

## Remaining Work

See [gap-register.md](gap-register.md). All app implementation gaps are currently open because this is the initial planning draft.
