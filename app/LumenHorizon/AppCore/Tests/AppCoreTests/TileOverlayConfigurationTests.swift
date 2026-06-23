//
//  TileOverlayConfigurationTests.swift
//  LumenHorizon
//
//  Created by Cedric Evrard on 6/21/26.
//

import Foundation
import Testing
@testable import AppCore

@Suite("Tile overlay configuration")
struct TileOverlayConfigurationTests {
    @Test("a valid png manifest produces a configuration")
    func validManifest() throws {
        let manifest = try Fixtures.overlayManifest(tileSetId: "ts-9")
        let config = try TileOverlayConfiguration(manifest: manifest)

        #expect(config.tileSetId == "ts-9")
        #expect(config.tileSize == 256)
        #expect(config.minZoom == 3)
        #expect(config.maxNativeZoom == 10)
        #expect(config.maxDisplayZoom == 12)
    }

    @Test("url construction substitutes z/x/y")
    func urlConstruction() throws {
        let manifest = try Fixtures.overlayManifest()
        let config = try TileOverlayConfiguration(manifest: manifest)

        #expect(
            config.template.url(z: 5, x: 9, y: 11)
                == "https://tiles.lumenhorizon.com/tiles/set/5/9/11.png"
        )
    }

    @Test("a non-png format is rejected")
    func nonPngRejected() throws {
        let manifest = try Fixtures.overlayManifest(format: "webp")
        #expect(throws: TileOverlayConfigurationError.unsupportedFormat("webp")) {
            _ = try TileOverlayConfiguration(manifest: manifest)
        }
    }

    @Test("a template missing placeholders is rejected")
    func missingPlaceholdersRejected() throws {
        let manifest = try Fixtures.overlayManifest(
            template: Fixtures.templateMissingPlaceholders
        )
        #expect(throws: TileURLTemplateError.self) {
            _ = try TileOverlayConfiguration(manifest: manifest)
        }
    }

    @Test("a non-positive tile size is rejected")
    func invalidTileSizeRejected() throws {
        let manifest = try Fixtures.overlayManifest(tileSize: 0)
        #expect(throws: TileOverlayConfigurationError.invalidTileSize(0)) {
            _ = try TileOverlayConfiguration(manifest: manifest)
        }
    }

    @Test("a non-monotonic zoom range is rejected")
    func invalidZoomRejected() throws {
        let manifest = try Fixtures.overlayManifest(minZoom: 11, maxNativeZoom: 10)
        #expect(throws: TileOverlayConfigurationError.self) {
            _ = try TileOverlayConfiguration(manifest: manifest)
        }
    }

    @Test("degenerate bounds are rejected")
    func invalidBoundsRejected() throws {
        // west == east => zero-width coverage.
        let manifest = try Fixtures.overlayManifest(west: -66.0, east: -66.0)
        #expect(throws: TileOverlayConfigurationError.self) {
            _ = try TileOverlayConfiguration(manifest: manifest)
        }
    }

    @Test("camera zoom range is derived from manifest zooms")
    func cameraZoomRangeUsesManifestZooms() throws {
        let manifest = try Fixtures.overlayManifest(
            tileSize: 256,
            minZoom: 3,
            maxNativeZoom: 10,
            maxDisplayZoom: 12,
            west: -1,
            south: -1,
            east: 1,
            north: 1
        )
        let config = try TileOverlayConfiguration(manifest: manifest)

        let range = try #require(config.cameraZoomRange)

        #expect(abs(range.minCenterCoordinateDistance - 9_783.93962) < 0.001)
        #expect(abs(range.maxCenterCoordinateDistance - 5_009_377.08544) < 0.001)
        #expect(range.minCenterCoordinateDistance < range.maxCenterCoordinateDistance)
    }

    @Test("camera zoom range honors max display zoom")
    func cameraZoomRangeUsesMaxDisplayZoomForCloseLimit() throws {
        let nativeOnlyManifest = try Fixtures.overlayManifest(maxNativeZoom: 10, maxDisplayZoom: 10)
        let upsampledManifest = try Fixtures.overlayManifest(maxNativeZoom: 10, maxDisplayZoom: 12)
        let nativeOnly = try TileOverlayConfiguration(manifest: nativeOnlyManifest)
        let upsampled = try TileOverlayConfiguration(manifest: upsampledManifest)

        let nativeOnlyRange = try #require(nativeOnly.cameraZoomRange)
        let upsampledRange = try #require(upsampled.cameraZoomRange)

        #expect(upsampledRange.minCenterCoordinateDistance < nativeOnlyRange.minCenterCoordinateDistance)
        #expect(upsampledRange.maxCenterCoordinateDistance == nativeOnlyRange.maxCenterCoordinateDistance)
    }

    @Test("camera zoom range changes with latitude")
    func cameraZoomRangeUsesBoundsCenterLatitude() throws {
        let equatorManifest = try Fixtures.overlayManifest(
            west: -1, south: -1, east: 1, north: 1
        )
        let highLatitudeManifest = try Fixtures.overlayManifest(
            west: -1, south: 59, east: 1, north: 61
        )
        let equator = try TileOverlayConfiguration(manifest: equatorManifest)
        let highLatitude = try TileOverlayConfiguration(manifest: highLatitudeManifest)

        let equatorRange = try #require(equator.cameraZoomRange)
        let highLatitudeRange = try #require(highLatitude.cameraZoomRange)

        #expect(highLatitudeRange.minCenterCoordinateDistance < equatorRange.minCenterCoordinateDistance)
        #expect(highLatitudeRange.maxCenterCoordinateDistance < equatorRange.maxCenterCoordinateDistance)
    }
}

@Suite("Map overlay state resolution")
struct MapOverlayStateResolutionTests {
    @Test("a valid manifest resolves to ready")
    func resolvesReady() throws {
        let manifest = try Fixtures.overlayManifest(tileSetId: "ready-1")
        let state = MapOverlayState.resolved(from: manifest)
        #expect(state.configuration?.tileSetId == "ready-1")
    }

    @Test("an invalid manifest resolves to unavailable(.invalidData)")
    func resolvesInvalidData() throws {
        let manifest = try Fixtures.overlayManifest(format: "jpeg")
        #expect(MapOverlayState.resolved(from: manifest) == .unavailable(.invalidData))
    }

    @Test(
        "not-found backend codes resolve to empty",
        arguments: [BackendErrorCode.notFound, .tileNotFound]
    )
    func resolvesEmpty(code: BackendErrorCode) {
        let error = BackendError.backend(code: code, message: "none", details: nil)
        #expect(MapOverlayState.resolved(from: error) == .empty)
    }

    @Test(
        "transient backend codes resolve to unavailable(.serverError)",
        arguments: [
            BackendErrorCode.serviceUnavailable, .tileUnavailable,
            .invalidRequest, .unknown("teapot")
        ]
    )
    func resolvesServerError(code: BackendErrorCode) {
        let error = BackendError.backend(code: code, message: "x", details: nil)
        #expect(MapOverlayState.resolved(from: error) == .unavailable(.serverError))
    }

    @Test("transport failures resolve to unavailable(.offline)")
    func resolvesOffline() {
        let error = BackendError.transport(message: "down")
        #expect(MapOverlayState.resolved(from: error) == .unavailable(.offline))
    }

    @Test("decoding failures resolve to unavailable(.invalidData)")
    func resolvesDecoding() {
        let error = BackendError.decoding(message: "bad")
        #expect(MapOverlayState.resolved(from: error) == .unavailable(.invalidData))
    }

    @Test("unexpected status resolves to unavailable(.serverError)")
    func resolvesUnexpectedStatus() {
        let error = BackendError.unexpectedStatus(code: 500)
        #expect(MapOverlayState.resolved(from: error) == .unavailable(.serverError))
    }

    @Test("a non-backend error resolves to unavailable(.serverError)")
    func resolvesUnknownError() {
        let error = URLError(.cannotParseResponse)
        #expect(MapOverlayState.resolved(from: error) == .unavailable(.serverError))
    }
}
