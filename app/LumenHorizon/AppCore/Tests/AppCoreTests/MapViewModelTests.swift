//
//  MapViewModelTests.swift
//  LumenHorizon
//
//  Created by Cedric Evrard on 6/21/26.
//

import Foundation
import Testing
@testable import AppCore

/// Serves a transport error on the first call, then a manifest, so retry
/// recovery can be exercised deterministically.
private actor RecoveringResponder {
    private var calls = 0
    let manifest: TileManifest
    init(manifest: TileManifest) { self.manifest = manifest }

    func next() throws -> TileManifest {
        calls += 1
        if calls == 1 { throw BackendError.transport(message: "offline") }
        return manifest
    }
}

private enum ManifestResponse: Sendable {
    case manifest(TileManifest)
    case error(BackendError)
}

private actor ManifestSequenceResponder {
    private var responses: [ManifestResponse]

    init(_ responses: [ManifestResponse]) {
        self.responses = responses
    }

    func next() throws -> TileManifest {
        switch responses.removeFirst() {
        case .manifest(let manifest):
            return manifest
        case .error(let error):
            throw error
        }
    }
}

private actor PausingResponder {
    private var calls = 0
    private var secondContinuation: CheckedContinuation<TileManifest, Never>?
    private var secondStartedContinuation: CheckedContinuation<Void, Never>?

    let firstManifest: TileManifest
    let secondManifest: TileManifest

    init(firstManifest: TileManifest, secondManifest: TileManifest) {
        self.firstManifest = firstManifest
        self.secondManifest = secondManifest
    }

    func next() async -> TileManifest {
        calls += 1
        if calls == 1 { return firstManifest }

        return await withCheckedContinuation { continuation in
            secondContinuation = continuation
            secondStartedContinuation?.resume()
            secondStartedContinuation = nil
        }
    }

    func waitForSecondCall() async {
        if secondContinuation != nil { return }

        await withCheckedContinuation { continuation in
            secondStartedContinuation = continuation
        }
    }

    func resumeSecond() {
        secondContinuation?.resume(returning: secondManifest)
        secondContinuation = nil
    }
}

@Suite("Map view model")
@MainActor
struct MapViewModelTests {
    @Test("starts idle")
    func startsIdle() throws {
        let manifest = try Fixtures.overlayManifest()
        let viewModel = MapViewModel { manifest }
        #expect(viewModel.state == .idle)
    }

    @Test("load success transitions to ready")
    func loadSuccess() async throws {
        let manifest = try Fixtures.overlayManifest(tileSetId: "ok-1")
        let viewModel = MapViewModel { manifest }
        await viewModel.load()
        #expect(viewModel.state.configuration?.tileSetId == "ok-1")
        #expect(viewModel.renderableConfiguration?.tileSetId == "ok-1")
    }

    @Test("a not-found manifest transitions to empty")
    func loadEmpty() async {
        let viewModel = MapViewModel {
            throw BackendError.backend(code: .notFound, message: "none", details: nil)
        }
        await viewModel.load()
        #expect(viewModel.state == .empty)
    }

    @Test("a transport failure transitions to unavailable(.offline)")
    func loadOffline() async {
        let viewModel = MapViewModel {
            throw BackendError.transport(message: "down")
        }
        await viewModel.load()
        #expect(viewModel.state == .unavailable(.offline))
    }

    @Test("retry recovers from a transient failure")
    func retryRecovers() async throws {
        let manifest = try Fixtures.overlayManifest(tileSetId: "recovered")
        let responder = RecoveringResponder(manifest: manifest)
        let viewModel = MapViewModel { try await responder.next() }

        await viewModel.load()
        #expect(viewModel.state == .unavailable(.offline))

        await viewModel.retry()
        #expect(viewModel.state.configuration?.tileSetId == "recovered")
    }

    @Test("renderable configuration is retained while a reload is loading")
    func renderableConfigurationRetainedDuringReload() async throws {
        let manifest = try Fixtures.overlayManifest(tileSetId: "stable")
        let responder = PausingResponder(
            firstManifest: manifest,
            secondManifest: manifest
        )
        let viewModel = MapViewModel { await responder.next() }

        await viewModel.load()
        #expect(viewModel.state.configuration?.tileSetId == "stable")
        #expect(viewModel.renderableConfiguration?.tileSetId == "stable")

        let reloadTask = Task { await viewModel.load() }
        await responder.waitForSecondCall()

        #expect(viewModel.state == .loading)
        #expect(viewModel.renderableConfiguration?.tileSetId == "stable")

        await responder.resumeSecond()
        await reloadTask.value

        #expect(viewModel.state.configuration?.tileSetId == "stable")
        #expect(viewModel.renderableConfiguration?.tileSetId == "stable")
    }

    @Test("renderable configuration is replaced when a new tile set resolves")
    func renderableConfigurationReplaced() async throws {
        let first = try Fixtures.overlayManifest(tileSetId: "old")
        let second = try Fixtures.overlayManifest(tileSetId: "new")
        let responder = ManifestSequenceResponder([
            .manifest(first),
            .manifest(second)
        ])
        let viewModel = MapViewModel { try await responder.next() }

        await viewModel.load()
        #expect(viewModel.renderableConfiguration?.tileSetId == "old")

        await viewModel.load()
        #expect(viewModel.state.configuration?.tileSetId == "new")
        #expect(viewModel.renderableConfiguration?.tileSetId == "new")
    }

    @Test("renderable configuration clears when latest manifest is empty")
    func renderableConfigurationClearsWhenEmpty() async throws {
        let manifest = try Fixtures.overlayManifest(tileSetId: "old")
        let responder = ManifestSequenceResponder([
            .manifest(manifest),
            .error(.backend(code: .notFound, message: "none", details: nil))
        ])
        let viewModel = MapViewModel { try await responder.next() }

        await viewModel.load()
        #expect(viewModel.renderableConfiguration?.tileSetId == "old")

        await viewModel.load()
        #expect(viewModel.state == .empty)
        #expect(viewModel.renderableConfiguration == nil)
    }

    @Test("renderable configuration clears when latest manifest is unavailable")
    func renderableConfigurationClearsWhenUnavailable() async throws {
        let manifest = try Fixtures.overlayManifest(tileSetId: "old")
        let responder = ManifestSequenceResponder([
            .manifest(manifest),
            .error(.transport(message: "offline"))
        ])
        let viewModel = MapViewModel { try await responder.next() }

        await viewModel.load()
        #expect(viewModel.renderableConfiguration?.tileSetId == "old")

        await viewModel.load()
        #expect(viewModel.state == .unavailable(.offline))
        #expect(viewModel.renderableConfiguration == nil)
    }

    @Test("opacity is clamped to 0...1")
    func opacityClamped() throws {
        let manifest = try Fixtures.overlayManifest()
        let viewModel = MapViewModel(initialOpacity: 2.0) { manifest }
        #expect(viewModel.opacity == 1.0)

        viewModel.opacity = -0.5
        #expect(viewModel.opacity == 0.0)
    }

    @Test("opacity is preserved across loads")
    func opacityPreserved() async throws {
        let manifest = try Fixtures.overlayManifest()
        let viewModel = MapViewModel(initialOpacity: 0.85) { manifest }
        viewModel.opacity = 0.3
        await viewModel.load()
        #expect(viewModel.opacity == 0.3)
    }
}
