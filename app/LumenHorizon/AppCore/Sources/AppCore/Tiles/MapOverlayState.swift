//
//  MapOverlayState.swift
//  LumenHorizon
//
//  Created by Cedric Evrard on 6/21/26.
//

import Foundation

/// The renderable state of the dark-sky tile overlay.
///
/// Pure and MapKit-free so overlay state transitions can be unit-tested
/// without networked map rendering. The view model drives this; the
/// SwiftUI map host eads it.
public enum MapOverlayState: Equatable, Sendable {
    /// Before the first load has started.
    case idle
    /// A manifest fetch is in flight.
    case loading
    /// A valid overlay can be rendered.
    case ready(TileOverlayConfiguration)
    /// The backend is healthy but no latest manifest is published yet.
    case empty
    /// A retryable failure occurred and no overlay can be rendered.
    case unavailable(MapOverlayUnavailableReason)
}

/// Why the overlay is unavailable. Every reason is retryable.
public enum MapOverlayUnavailableReason: Equatable, Sendable {
    /// The request never reached the backend (no connection, timeout).
    case offline
    /// The backend responded with a transient or unexpected failure.
    case serverError
    /// The manifest could not be decoded or failed overlay validation.
    case invalidData
}

public extension MapOverlayState {
    /// The configuration when an overlay can render, otherwise `nil`.
    var configuration: TileOverlayConfiguration? {
        if case .ready(let configuration) = self { return configuration }
        return nil
    }

    /// Whether a manifest fetch is currently in flight.
    var isLoading: Bool {
        if case .loading = self { return true }
        return false
    }

    // MARK: - Resolution

    /// Resolves a successfully fetched manifest into a state.
    ///
    /// A manifest that fails `TileOverlayConfiguration` validation is funneled
    /// to `.unavailable(.invalidData)` so a malformed manifest never renders.
    static func resolved(from manifest: TileManifest) -> MapOverlayState {
        do {
            return .ready(try TileOverlayConfiguration(manifest: manifest))
        } catch {
            return .unavailable(.invalidData)
        }
    }

    static func resolved(from error: Error) -> MapOverlayState {
        guard let backendError = error as? BackendError else {
            return .unavailable(.serverError)
        }

        switch backendError {
        case .transport:
            return .unavailable(.offline)

        case .decoding:
            return .unavailable(.invalidData)

        case .unexpectedStatus, .missingData:
            return .unavailable(.serverError)

        case .backend(let code, _, _):
            switch code {
            case .notFound, .tileNotFound:
                return .empty
            case .serviceUnavailable, .tileUnavailable, .invalidRequest, .unknown:
                return .unavailable(.serverError)
            }
        }
    }
}
