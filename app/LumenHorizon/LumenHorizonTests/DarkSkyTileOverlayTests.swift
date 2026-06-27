//
//  DarkSkyTileOverlayTests.swift
//  LumenHorizonTests
//
//  Created by Copilot on 6/22/26.
//

import AppCore
import CoreGraphics
import Foundation
import ImageIO
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

private struct RGBAColor: Equatable {
    let red: UInt8
    let green: UInt8
    let blue: UInt8
    let alpha: UInt8
}

private func pngData(
    width: Int,
    height: Int,
    pixels: [RGBAColor]
) throws -> Data {
    let bytesPerPixel = 4
    let bytesPerRow = width * bytesPerPixel
    var bytes = pixels.flatMap { [$0.red, $0.green, $0.blue, $0.alpha] }
    let colorSpace = try #require(CGColorSpace(name: CGColorSpace.sRGB))
    let context = try #require(CGContext(
        data: &bytes,
        width: width,
        height: height,
        bitsPerComponent: 8,
        bytesPerRow: bytesPerRow,
        space: colorSpace,
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
    ))
    let image = try #require(context.makeImage())
    let data = NSMutableData()
    let destination = try #require(CGImageDestinationCreateWithData(
        data,
        "public.png" as CFString,
        1,
        nil
    ))

    CGImageDestinationAddImage(destination, image, nil)
    #expect(CGImageDestinationFinalize(destination))

    return data as Data
}

private func decodedPixels(from data: Data) throws -> [RGBAColor] {
    let source = try #require(CGImageSourceCreateWithData(data as CFData, nil))
    let image = try #require(CGImageSourceCreateImageAtIndex(source, 0, nil))
    let width = image.width
    let height = image.height
    let bytesPerPixel = 4
    let bytesPerRow = width * bytesPerPixel
    var bytes = [UInt8](repeating: 0, count: height * bytesPerRow)
    let colorSpace = try #require(CGColorSpace(name: CGColorSpace.sRGB))
    let context = try #require(CGContext(
        data: &bytes,
        width: width,
        height: height,
        bitsPerComponent: 8,
        bytesPerRow: bytesPerRow,
        space: colorSpace,
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
    ))

    context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))

    return stride(from: 0, to: bytes.count, by: bytesPerPixel).map { index in
        RGBAColor(
            red: bytes[index],
            green: bytes[index + 1],
            blue: bytes[index + 2],
            alpha: bytes[index + 3]
        )
    }
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

    @Test("display zoom is exposed to MapKit for overzoom rendering")
    func displayZoomIsExposedToMapKit() throws {
        let overlay = try Self.makeOverlay { _ in
            throw TileLoaderTestError.unexpectedNetworkCall
        }

        #expect(overlay.minimumZ == 3)
        #expect(overlay.maximumZ == 12)
    }

    @Test("display zoom tile requests fetch native parent tiles")
    func displayZoomTileRequestsFetchNativeParentTiles() async throws {
        let png = try pngData(
            width: 4,
            height: 4,
            pixels: Array(repeating: RGBAColor(red: 20, green: 40, blue: 60, alpha: 255), count: 16)
        )
        var requestedURL: URL?
        let overlay = try Self.makeOverlay { request in
            requestedURL = request.url
            let response = try #require(makeHTTPResponse(
                url: request.url,
                statusCode: 200,
                contentType: "image/png"
            ))
            return (png, response)
        }

        let result = await Self.loadTile(
            from: overlay,
            path: MKTileOverlayPath(x: 19, y: 23, z: 11, contentScaleFactor: 1)
        )

        #expect(requestedURL?.absoluteString == "https://tiles.lumenhorizon.com/tiles/set/10/9/11.png")
        #expect(result.data?.isEmpty == false)
        #expect(result.error == nil)
    }

    @Test("display zoom tiles crop and upscale the requested child quadrant")
    func displayZoomTilesCropAndUpscaleChildQuadrant() async throws {
        let topLeft = RGBAColor(red: 255, green: 0, blue: 0, alpha: 255)
        let topRight = RGBAColor(red: 0, green: 255, blue: 0, alpha: 255)
        let bottomLeft = RGBAColor(red: 0, green: 0, blue: 255, alpha: 255)
        let bottomRight = RGBAColor(red: 255, green: 255, blue: 0, alpha: 255)
        let png = try pngData(
            width: 4,
            height: 4,
            pixels: [
                topLeft, topLeft, topRight, topRight,
                topLeft, topLeft, topRight, topRight,
                bottomLeft, bottomLeft, bottomRight, bottomRight,
                bottomLeft, bottomLeft, bottomRight, bottomRight,
            ]
        )
        let overlay = try Self.makeOverlay { request in
            let response = try #require(makeHTTPResponse(
                url: request.url,
                statusCode: 200,
                contentType: "image/png"
            ))
            return (png, response)
        }

        let result = await Self.loadTile(
            from: overlay,
            path: MKTileOverlayPath(x: 19, y: 23, z: 11, contentScaleFactor: 1)
        )
        let data = try #require(result.data)
        let pixels = try decodedPixels(from: data)

        #expect(pixels.allSatisfy { $0 == bottomRight })
        #expect(result.error == nil)
    }

    @Test("display zoom child tiles reuse cached native parent bytes")
    func displayZoomChildTilesReuseCachedNativeParentBytes() async throws {
        let png = try pngData(
            width: 4,
            height: 4,
            pixels: Array(repeating: RGBAColor(red: 80, green: 90, blue: 100, alpha: 255), count: 16)
        )
        var requestCount = 0
        let overlay = try Self.makeOverlay { request in
            requestCount += 1
            let response = try #require(makeHTTPResponse(
                url: request.url,
                statusCode: 200,
                contentType: "image/png"
            ))
            return (png, response)
        }

        _ = await Self.loadTile(
            from: overlay,
            path: MKTileOverlayPath(x: 18, y: 22, z: 11, contentScaleFactor: 1)
        )
        _ = await Self.loadTile(
            from: overlay,
            path: MKTileOverlayPath(x: 19, y: 23, z: 11, contentScaleFactor: 1)
        )

        #expect(requestCount == 1)
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
        from overlay: DarkSkyTileOverlay,
        path: MKTileOverlayPath = MKTileOverlayPath(x: 9, y: 11, z: 5, contentScaleFactor: 1)
    ) async -> (data: Data?, error: (any Error)?) {
        await withCheckedContinuation { continuation in
            overlay.loadTile(at: path) {
                data,
                error in
                continuation.resume(returning: (data, error))
            }
        }
    }

}
