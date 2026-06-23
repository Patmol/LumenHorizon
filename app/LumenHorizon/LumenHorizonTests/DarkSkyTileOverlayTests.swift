//
//  DarkSkyTileOverlayTests.swift
//  LumenHorizonTests
//
//  Created by Copilot on 6/22/26.
//

import AppCore
import Foundation
import MapKit
import Testing
@testable import LumenHorizon

private struct StubTileDataLoader: HTTPDataFetching {
    let load: @Sendable (URLRequest) async throws -> (Data, URLResponse)

    func data(for request: URLRequest) async throws -> (Data, URLResponse) {
        try await load(request)
    }
}

private enum TileLoaderTestError: Error {
    case unexpectedNetworkCall
    case transport
}

private let validTileTemplate =
    "https://tiles.lumenhorizon.com/tiles/set/{z}/{x}/{y}.png"

private func overlayManifest(template: String) throws -> TileManifest {
    let json = """
    {
      "tile_set_id": "ts-overlay-test",
      "dataset_date": "2026-05-21",
      "generated_at": "2026-05-21T09:15:00Z",
      "classification_version": "radiance-dark-sky-v1",
      "render_version": "tiles-v1",
      "format": "png",
      "tile_size": 256,
      "min_zoom": 3,
      "max_native_zoom": 10,
      "max_display_zoom": 12,
      "bounds": { "west": -125.0, "south": 24.0, "east": -66.0, "north": 50.0 },
      "tile_url_template": "\(template)",
      "tile_count": 12345,
      "checksums": { "manifest_sha256": "abc123" }
    }
    """
    return try JSONDecoder().decode(TileManifest.self, from: Data(json.utf8))
}

private func makeHTTPResponse(
    url: URL?,
    statusCode: Int,
    contentType: String
) -> HTTPURLResponse? {
    guard let url else { return nil }
    return HTTPURLResponse(
        url: url,
        statusCode: statusCode,
        httpVersion: nil,
        headerFields: ["Content-Type": contentType]
    )
}

@Suite("Dark sky tile overlay loading")
@MainActor
struct DarkSkyTileOverlayTests {
    @Test("valid PNG tile bytes pass through unchanged")
    func validPNGTilePassesThrough() async throws {
        let png = Data([0x89, 0x50, 0x4E, 0x47])
        let overlay = try Self.makeOverlay { request in
            let response = try #require(makeHTTPResponse(
                url: request.url,
                statusCode: 200,
                contentType: "image/png"
            ))
            return (png, response)
        }

        let result = await Self.loadTile(from: overlay)

        #expect(result.data == png)
        #expect(result.error == nil)
    }

    @Test("HTTP 404 returns transparent no-data")
    func notFoundReturnsTransparentNoData() async throws {
        let overlay = try Self.makeOverlay { request in
            let response = try #require(makeHTTPResponse(
                url: request.url,
                statusCode: 404,
                contentType: "application/xml"
            ))
            return (Data("<Error>BlobNotFound</Error>".utf8), response)
        }

        let result = await Self.loadTile(from: overlay)

        #expect(result.data == Data())
        #expect(result.error == nil)
    }

    @Test("non-PNG content returns transparent no-data")
    func nonPNGReturnsTransparentNoData() async throws {
        let overlay = try Self.makeOverlay { request in
            let response = try #require(makeHTTPResponse(
                url: request.url,
                statusCode: 200,
                contentType: "text/html; charset=utf-8"
            ))
            return (Data("<html></html>".utf8), response)
        }

        let result = await Self.loadTile(from: overlay)

        #expect(result.data == Data())
        #expect(result.error == nil)
    }

    @Test("empty PNG response returns transparent no-data")
    func emptyPNGReturnsTransparentNoData() async throws {
        let overlay = try Self.makeOverlay { request in
            let response = try #require(makeHTTPResponse(
                url: request.url,
                statusCode: 200,
                contentType: "image/png"
            ))
            return (Data(), response)
        }

        let result = await Self.loadTile(from: overlay)

        #expect(result.data == Data())
        #expect(result.error == nil)
    }

    @Test("transport errors return transparent no-data")
    func transportErrorReturnsTransparentNoData() async throws {
        let overlay = try Self.makeOverlay { _ in
            throw TileLoaderTestError.transport
        }

        let result = await Self.loadTile(from: overlay)

        #expect(result.data == Data())
        #expect(result.error == nil)
    }

    @Test("unsupported tile URLs return transparent no-data without fetching")
    func unsupportedTileURLReturnsTransparentNoData() async throws {
        let overlay = try Self.makeOverlay(template: "ftp://tiles.example.com/{z}/{x}/{y}.png") { _ in
            throw TileLoaderTestError.unexpectedNetworkCall
        }

        let result = await Self.loadTile(from: overlay)

        #expect(result.data == Data())
        #expect(result.error == nil)
    }

    private static func makeOverlay(
        template: String = validTileTemplate,
        load: @escaping @Sendable (URLRequest) async throws -> (Data, URLResponse)
    ) throws -> DarkSkyTileOverlay {
        let manifest = try overlayManifest(template: template)
        let configuration = try TileOverlayConfiguration(manifest: manifest)
        return DarkSkyTileOverlay(
            configuration: configuration,
            tileDataLoader: StubTileDataLoader(load: load)
        )
    }

    private static func loadTile(
        from overlay: DarkSkyTileOverlay
    ) async -> (data: Data?, error: (any Error)?) {
        await withCheckedContinuation { continuation in
            overlay.loadTile(at: MKTileOverlayPath(x: 9, y: 11, z: 5, contentScaleFactor: 1)) {
                data,
                error in
                continuation.resume(returning: (data, error))
            }
        }
    }

}
