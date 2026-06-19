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

## Draft Local Commands

The exact commands should be finalized when the Xcode project exists.

```bash
xcodebuild -scheme LumenHorizon-iOS -destination 'platform=iOS Simulator,name=iPhone 16' build test
xcodebuild -scheme LumenHorizon-macOS -destination 'platform=macOS' build test
xcodebuild -scheme LumenHorizon-visionOS -destination 'platform=visionOS Simulator,name=Apple Vision Pro' build test
```

The visionOS check requires an installed visionOS SDK and simulator runtime; skip it only with an explicit note in the chunk handoff when that runtime is unavailable.

If the app uses a shared Swift package:

```bash
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

Draft CI should include:

- Build and test the shared app package or app core target.
- Build and test the iOS target on a simulator.
- Build and test the macOS target.
- Build and test the visionOS target on a simulator when the CI runner provides the required SDK and runtime.
- Run UI smoke tests if runtime and simulator stability are acceptable.

CI must not require private backend credentials or a live API Gateway. Contract fixtures should cover normal and error envelope shapes.

## Validation Standard

Before marking app work complete, run the narrowest useful check while developing and then the broader app validation for shared or user-visible changes. If a check requires Xcode, simulator runtime, or a local backend state that is unavailable, document the skipped check and the reason in the chunk handoff.
