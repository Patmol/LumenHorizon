# 30. Local Development And CI

## Goals

- Keep a clean clone easy to build on macOS with Xcode.
- Keep local app validation and CI aligned.
- Validate app behavior against fixtures without requiring a live backend.
- Support an optional local smoke path against the backend API Gateway.

## Expected Local Tools

| Tool | Purpose |
| --- | --- |
| Xcode | Build and run iOS, macOS, and visionOS targets, simulators, SwiftUI previews, and UI tests. |
| Swift Package Manager | Build and test shared app packages if the project uses package boundaries. |
| `xcodebuild` | Non-interactive local and CI validation. |
| `just` | Optional repository-level wrapper for app and backend smoke commands. |

## Local Build Commands

The app currently uses one shared `LumenHorizon` scheme with platform-specific destinations:

```bash
xcodebuild -project app/LumenHorizon/LumenHorizon.xcodeproj -scheme LumenHorizon -destination 'generic/platform=iOS Simulator' CODE_SIGNING_ALLOWED=NO build
xcodebuild -project app/LumenHorizon/LumenHorizon.xcodeproj -scheme LumenHorizon -destination 'generic/platform=macOS' CODE_SIGNING_ALLOWED=NO build
xcodebuild -project app/LumenHorizon/LumenHorizon.xcodeproj -scheme LumenHorizon -destination 'generic/platform=visionOS Simulator' CODE_SIGNING_ALLOWED=NO build
```

The visionOS check requires an installed Xcode version with the visionOS SDK. Skip it only with an explicit note in the chunk handoff when that SDK is unavailable.

Simulator-backed app unit and UI tests are deferred until the shared scheme or test plan can run them without making the build-only CI path flaky. AppCore package tests can run without simulator boot.

The shared app package can be built directly when app core changes:

```bash
cd app/LumenHorizon/AppCore
swift build
swift test
```

Optional repository-level wrappers can be added after the project exists:

```bash
just app-check
just app-test
just app-smoke-local
```

## Local Backend Smoke

The app should be testable without a live backend, but a manual smoke path should verify the real contract:

```bash
just up
just migrate
just serve-api
```

Then launch the app with its debug API base URL pointed at the local API Gateway. The backend must have a published latest tile manifest for the map overlay to render real PNG tiles.

## CI Jobs

CI includes:

- Build the iOS target for a generic simulator destination.
- Build the macOS target.
- Build the visionOS target for a generic simulator destination when the CI runner provides the required SDK.
- Run AppCore package tests after the app build job succeeds.

Future CI expansion should include:

- Simulator-backed app target unit tests once they are stable in CI.
- UI smoke tests if runtime and simulator stability are acceptable.

CI must not require private backend credentials or a live API Gateway. Contract fixtures should cover normal and error envelope shapes.

## Validation Standard

Before marking app work complete, run the narrowest useful check while developing and then the broader app validation for shared or user-visible changes. If a check requires Xcode, simulator runtime, or a local backend state that is unavailable, document the skipped check and the reason in the chunk handoff.
