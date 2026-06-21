//
//  ContractDecodingTests.swift
//  LumenHorizon
//
//  Created by Cedric Evrard on 6/20/26.
//

import Foundation
import Testing
@testable import AppCore

@Suite("Contract decoding")
struct ContractDecodingTests {
    private let decoder = JSONDecoder()

    @Test("latest manifest envelope decodes with bounds and checksums")
    func decodesManifestEnvelope() throws {
        let envelope = try decoder.decode(
            ApiEnvelope<TileManifest>.self,
            from: Data(Fixtures.latestManifestEnvelope.utf8)
        )
        let manifest = try #require(envelope.data)

        #expect(manifest.tileSetId == "2026-05-21-radiance-dark-sky-v1-00000000-a1")
        #expect(manifest.format == "png")
        #expect(manifest.tileSize == 256)
        #expect(manifest.minZoom == 3)
        #expect(manifest.maxNativeZoom == 10)
        #expect(manifest.maxDisplayZoom == 12)
        #expect(manifest.bounds == TileBounds(west: -125, south: 24, east: -66, north: 50))
        #expect(manifest.checksums.manifestSHA256 == "abc123")
        #expect(envelope.error == nil)
        #expect(envelope.meta?.requestId == "req-1")
    }

    @Test("manifest tolerates unknown additive fields")
    func toleratesUnknownFields() throws {
        // processor_version and source_granules are present but undeclared.
        let envelope = try decoder.decode(
            ApiEnvelope<TileManifest>.self,
            from: Data(Fixtures.manifestByIdEnvelope.utf8)
        )
        #expect(envelope.data?.tileSetId == "2026-04-01-radiance-dark-sky-v1-00000000-b2")
    }

    @Test("malformed manifest missing tile_url_template fails to decode")
    func malformedManifestFailsDecode() {
        #expect(throws: DecodingError.self) {
            _ = try decoder.decode(
                ApiEnvelope<TileManifest>.self,
                from: Data(Fixtures.manifestMissingTemplateEnvelope.utf8)
            )
        }
    }

    @Test("tile classes decode and preserve legend ordering")
    func decodesTileClasses() throws {
        let envelope = try decoder.decode(
            ApiEnvelope<TileClasses>.self,
            from: Data(Fixtures.tileClassesEnvelope.utf8)
        )
        let classes = try #require(envelope.data)

        #expect(classes.classificationVersion == "radiance-dark-sky-v1")
        #expect(classes.radianceUnits == "nW/cm^2/sr")
        #expect(classes.classes.map(\.classNumber) == [1, 2])
        #expect(classes.classes.first?.colorHex == "#05070d")
        #expect(classes.classes.first?.maxRadianceExclusive == 0.2)
    }
}

@Suite("Tile URL template")
struct TileURLTemplateTests {
    @Test("valid template substitutes z/x/y coordinates")
    func substitutesCoordinates() throws {
        let template = try TileURLTemplate(Fixtures.validTemplate)
        #expect(
            template.url(z: 3, x: 1, y: 2)
                == "https://tiles.lumenhorizon.com/tiles/set/3/1/2.png"
        )
    }

    @Test("missing placeholders are rejected at construction")
    func rejectsMissingPlaceholders() {
        #expect(throws: TileURLTemplateError.missingPlaceholders(["{x}", "{y}"])) {
            _ = try TileURLTemplate(Fixtures.templateMissingPlaceholders)
        }
    }
}
