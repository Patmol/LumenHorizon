//
//  MapViewModel.swift
//  LumenHorizon
//
//  Created by Cedric Evrard on 6/21/26.
//

import Foundation
import Observation

/// Drives the dark-sky tile overlay: loads the latest manifest, maps the
/// result into a `MapOverlayState`, and owns the overlay opacity.
///
/// State transitions live in `MapOverlayState`; this type only orchestrates
/// the fetch and exposes observable state for SwiftUI. A manifest-loading
/// closure is injected so transitions can be tested without a live network.
@MainActor
@Observable
public final class MapViewModel {
    /// Default overlay opacity (dark-sky layer over the base map).
    public static let defaultOpacity = 0.85

    /// Current renderable overlay state.
    public private(set) var state: MapOverlayState = .idle

    /// The configuration the map should render, including during refresh.
    public var renderableConfiguration: TileOverlayConfiguration? {
        state.configuration ?? (state.isLoading ? lastResolvedConfiguration : nil)
    }

    /// Overlay opacity in `0...1`. Survives reloads and retries.
    public var opacity: Double {
        didSet {
            let clamped = min(max(opacity, 0), 1)
            if clamped != opacity { opacity = clamped }
        }
    }

    private let loadManifest: @Sendable () async throws -> TileManifest
    private var lastResolvedConfiguration: TileOverlayConfiguration?

    /// Designated initializer with an injectable manifest loader (tests/previews).
    public init(
        initialOpacity: Double = MapViewModel.defaultOpacity,
        loadManifest: @escaping @Sendable () async throws -> TileManifest
    ) {
        self.opacity = min(max(initialOpacity, 0), 1)
        self.loadManifest = loadManifest
    }

    /// Convenience initializer backed by a configured `BackendClient`.
    public convenience init(
        client: BackendClient,
        initialOpacity: Double = MapViewModel.defaultOpacity
    ) {
        self.init(initialOpacity: initialOpacity) {
            try await client.latestManifest()
        }
    }

    /// Fetches the latest manifest and resolves the overlay state.
    ///
    /// Sets `.loading`, then exactly one of `.ready` / `.empty` /
    /// `.unavailable` per the Step 1 contract. Opacity is never reset.
    public func load() async {
        state = .loading
        do {
            let manifest = try await loadManifest()
            applyResolvedState(.resolved(from: manifest))
        } catch {
            applyResolvedState(.resolved(from: error))
        }
    }

    /// Re-runs `load()`. Used by the retry affordance in the UI.
    public func retry() async {
        await load()
    }

    private func applyResolvedState(_ resolvedState: MapOverlayState) {
        state = resolvedState
        lastResolvedConfiguration = resolvedState.configuration
    }
}
