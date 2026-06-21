//
//  BackendClientTests.swift
//  LumenHorizon
//
//  Created by Cedric Evrard on 6/20/26.
//

import Foundation
import Testing
@testable import AppCore

@Suite("Backend client")
struct BackendClientTests {
    private func makeClient(_ session: StubHTTPClient) throws -> BackendClient {
        let api = try APIConfiguration(
            baseURL: #require(URL(string: "https://api.lumenhorizon.app")),
            requestTimeout: 15
        )
        return BackendClient(api: api, session: session)
    }

    @Test("latestManifest decodes a success envelope")
    func latestManifestSucceeds() async throws {
        let client = try makeClient(StubHTTPClient(json: Fixtures.latestManifestEnvelope))
        let manifest = try await client.latestManifest()
        #expect(manifest.classificationVersion == "radiance-dark-sky-v1")
    }

    @Test(
        "backend error envelopes map to typed codes",
        arguments: [
            ("invalid_request", BackendErrorCode.invalidRequest, 400),
            ("not_found", .notFound, 404),
            ("tile_unavailable", .tileUnavailable, 503),
            ("tile_not_found", .tileNotFound, 404),
            ("service_unavailable", .serviceUnavailable, 503)
        ]
    )
    func mapsBackendErrors(
        wire: String,
        expected: BackendErrorCode,
        status: Int
    ) async throws {
        let client = try makeClient(
            StubHTTPClient(
                json: Fixtures.errorEnvelope(code: wire, message: "boom"),
                statusCode: status
            )
        )

        await #expect(throws: BackendError.backend(code: expected, message: "boom", details: nil)) {
            _ = try await client.latestManifest()
        }
    }

    @Test("unknown backend code falls back to .unknown")
    func mapsUnknownCode() async throws {
        let client = try makeClient(
            StubHTTPClient(
                json: Fixtures.errorEnvelope(code: "teapot", message: "no coffee"),
                statusCode: 418
            )
        )
        await #expect(throws: BackendError.backend(code: .unknown("teapot"), message: "no coffee", details: nil)) {
            _ = try await client.latestManifest()
        }
    }

    @Test("tile sets surface next_cursor when present")
    func tileSetsWithCursor() async throws {
        let client = try makeClient(StubHTTPClient(json: Fixtures.tileSetsWithCursorEnvelope))
        let page = try await client.tileSets(limit: 1, cursor: nil)

        #expect(page.sets.map(\.tileSetId) == ["set-a"])
        #expect(page.sets.first?.latest == true)
        #expect(page.nextCursor == "opaque-cursor-xyz")
    }

    @Test("tile sets report nil cursor on the last page")
    func tileSetsWithoutCursor() async throws {
        let client = try makeClient(StubHTTPClient(json: Fixtures.tileSetsWithoutCursorEnvelope))
        let page = try await client.tileSets(limit: 1, cursor: "opaque-cursor-xyz")

        #expect(page.nextCursor == nil)
        #expect(page.sets.first?.latest == false)
    }

    @Test("transport failures surface as .transport")
    func transportFailure() async throws {
        let client = try makeClient(StubHTTPClient(throwing: URLError(.notConnectedToInternet)))
        await #expect(throws: BackendError.self) {
            _ = try await client.latestManifest()
        }
    }
}

@Suite("Cache metadata")
struct CacheMetadataTests {
    @Test("classes metadata matches a manifest with the same classification version")
    func classesMetadataMatchesManifest() throws {
        let decoder = JSONDecoder()
        let manifest = try #require(
            decoder.decode(
                ApiEnvelope<TileManifest>.self,
                from: Data(Fixtures.latestManifestEnvelope.utf8)
            ).data
        )
        let classes = try #require(
            decoder.decode(
                ApiEnvelope<TileClasses>.self,
                from: Data(Fixtures.tileClassesEnvelope.utf8)
            ).data
        )

        let metadata = TileClassesCacheMetadata(classes: classes, fetchedAt: .now)
        #expect(metadata.matches(manifest))
    }
}
