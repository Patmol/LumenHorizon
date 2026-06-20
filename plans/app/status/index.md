# App Status Overview

## Current Snapshot

The app plan now has a working native project foundation. A multiplatform Xcode project targets iOS, macOS, and visionOS from shared code, and the `AppCore` Swift package provides environment-driven app configuration and map defaults. The backend already exposes the anonymous tile manifest, tile classes, tile-set list, and PNG tile redirect/template contracts needed for a MapKit client.

## Already Done

- App planning structure exists under `plans/app`.
- Backend client contracts are identified from existing backend docs and code.
- Initial chunk roadmap, architecture, standards, integration, testing, and gap docs are drafted.
- Chunk 1 (Xcode project and shared app foundation) is complete:
  - Multiplatform Xcode project with iOS, macOS, and visionOS support, building in Debug and Release.
  - Shared `AppCore` Swift package with `AppConfiguration`, `APIConfiguration`, `AppEnvironment`, and `MapDefaults`.
  - Environment-driven API base URLs for local, preview, and release, wired through build settings and `Info.plist` into `AppRuntimeConfiguration`, with a guard preventing release builds from pointing at a local backend.
  - Basic SwiftUI app shell with navigation, a MapKit map, and platform-specific window/scene setup.
  - `AppCore` unit tests covering configuration parsing and validation.

## Not Started

- Backend API client.
- MapKit tile overlay.
- Legend and dataset metadata UI.
- Tile-set selection.
- App UI tests and CI.
- App developer guide.

## Current Roadmap Position

The active app roadmap is at [../00-implementation-roadmap.md](../00-implementation-roadmap.md). Chunks 0-1 (architecture baseline and project/shared foundation) are complete. Chunk 2 (backend client and contract models) is the next chunk. Chunks 3-6 build the map experience, legend, tile-set selection, and cache resilience. Chunks 7-9 cover native polish, quality gates, and local launch readiness.

## Remaining Work

See [gap-register.md](gap-register.md). Project foundation (APP-001) is closed; the remaining app implementation gaps are open from Chunk 2 onward.
