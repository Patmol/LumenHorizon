//
//  DarkSkyTileOverlay.swift
//  LumenHorizon
//
//  Created by Cedric Evrard on 6/21/26.
//

import AppCore
import Foundation
import ImageIO
import MapKit

/// An `MKTileOverlay` that renders the backend dark-sky PNG tile set.
///
/// All identity, zoom, bounds, and URL behavior come from a validated
/// `TileOverlayConfiguration`, so this class only bridges that contract to
/// MapKit. Tile URLs are produced by the shared `TileURLTemplate`.
final class DarkSkyTileOverlay: MKTileOverlay {
    private static let transparentNoData = Data()
    private static let pngTypeIdentifier = "public.png" as CFString

    private struct TileRequest: Sendable {
        let z: Int
        let x: Int
        let y: Int
        let overzoom: Overzoom?
    }

    private struct Overzoom: Sendable {
        let scale: Int
        let offsetX: Int
        let offsetY: Int
    }

    /// The configuration this overlay renders. Used to detect stale overlays.
    let configuration: TileOverlayConfiguration

    private let tileDataLoader: any HTTPDataFetching
    private let tileDataCache = TileDataCache()

    init(
        configuration: TileOverlayConfiguration,
        tileDataLoader: any HTTPDataFetching = URLSession.shared
    ) {
        self.configuration = configuration
        self.tileDataLoader = tileDataLoader
        // Custom URL logic is provided via `url(forTilePath:)`, so no template.
        super.init(urlTemplate: nil)

        tileSize = CGSize(
            width: configuration.tileSize,
            height: configuration.tileSize
        )
        minimumZ = configuration.minZoom
        maximumZ = configuration.maxDisplayZoom
        // Dark-sky layer sits over the base map; never hide map content.
        canReplaceMapContent = false
    }

    /// Coverage extent so MapKit only requests tiles within the dataset bounds.
    override var boundingMapRect: MKMapRect {
        let bounds = configuration.bounds
        let topLeft = MKMapPoint(
            CLLocationCoordinate2D(latitude: bounds.north, longitude: bounds.west)
        )
        let bottomRight = MKMapPoint(
            CLLocationCoordinate2D(latitude: bounds.south, longitude: bounds.east)
        )
        return MKMapRect(
            x: min(topLeft.x, bottomRight.x),
            y: min(topLeft.y, bottomRight.y),
            width: abs(bottomRight.x - topLeft.x),
            height: abs(bottomRight.y - topLeft.y)
        )
    }

    /// Builds each tile URL from the validated template.
    override func url(forTilePath path: MKTileOverlayPath) -> URL {
        guard let tileRequest = tileRequest(for: path) else {
            return URL(fileURLWithPath: "/dev/null")
        }

        let urlString = configuration.template.url(
            z: tileRequest.z,
            x: tileRequest.x,
            y: tileRequest.y
        )
        guard let url = URL(string: urlString), Self.isSupportedTileURL(url) else {
            return URL(fileURLWithPath: "/dev/null")
        }
        return url
    }

    /// Fetches tile bytes and converts missing/non-image responses into no-data.
    override func loadTile(
        at path: MKTileOverlayPath,
        result: @escaping (Data?, (any Error)?) -> Void
    ) {
        guard let tileRequest = tileRequest(for: path) else {
            result(Self.transparentNoData, nil)
            return
        }

        let urlString = configuration.template.url(
            z: tileRequest.z,
            x: tileRequest.x,
            y: tileRequest.y
        )
        guard let url = URL(string: urlString), Self.isSupportedTileURL(url) else {
            result(Self.transparentNoData, nil)
            return
        }

        let request = URLRequest(url: url)
        Task { [tileDataLoader, tileDataCache, overzoom = tileRequest.overzoom] in
            do {
                let renderableData: Data
                if let cachedData = await tileDataCache.data(for: url) {
                    renderableData = cachedData
                } else {
                    let (data, response) = try await tileDataLoader.data(for: request)
                    renderableData = Self.renderableTileData(data, response: response)
                    if !renderableData.isEmpty {
                        await tileDataCache.store(renderableData, for: url)
                    }
                }

                guard !renderableData.isEmpty else {
                    result(Self.transparentNoData, nil)
                    return
                }

                if let overzoom {
                    result(Self.overzoomedTileData(renderableData, overzoom: overzoom), nil)
                } else {
                    result(renderableData, nil)
                }
            } catch {
                result(Self.transparentNoData, nil)
            }
        }
    }

    private func tileRequest(for path: MKTileOverlayPath) -> TileRequest? {
        guard path.z >= configuration.minZoom,
              path.z <= configuration.maxDisplayZoom,
              path.x >= 0,
              path.y >= 0 else {
            return nil
        }

        guard path.z > configuration.maxNativeZoom else {
            return TileRequest(z: path.z, x: path.x, y: path.y, overzoom: nil)
        }

        let zoomDelta = path.z - configuration.maxNativeZoom
        guard zoomDelta < Int.bitWidth - 1 else { return nil }

        let scale = 1 << zoomDelta
        let parentX = path.x / scale
        let parentY = path.y / scale

        return TileRequest(
            z: configuration.maxNativeZoom,
            x: parentX,
            y: parentY,
            overzoom: Overzoom(
                scale: scale,
                offsetX: path.x - parentX * scale,
                offsetY: path.y - parentY * scale
            )
        )
    }

    private static func isSupportedTileURL(_ url: URL) -> Bool {
        guard let scheme = url.scheme?.lowercased() else { return false }
        return scheme == "http" || scheme == "https"
    }

    private static func renderableTileData(_ data: Data, response: URLResponse) -> Data {
        guard !data.isEmpty,
              let http = response as? HTTPURLResponse,
              http.statusCode == 200,
              let contentType = http.value(forHTTPHeaderField: "Content-Type") else {
            return transparentNoData
        }

        let mediaType = contentType
            .split(separator: ";", maxSplits: 1)
            .first?
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased()

        guard mediaType == "image/png" else {
            return transparentNoData
        }

        return data
    }

    private static func overzoomedTileData(_ data: Data, overzoom: Overzoom) -> Data {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil),
              let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else {
            return transparentNoData
        }

        let cropWidth = image.width / overzoom.scale
        let cropHeight = image.height / overzoom.scale
        guard cropWidth > 0, cropHeight > 0 else { return transparentNoData }

        let cropRect = CGRect(
            x: overzoom.offsetX * cropWidth,
            y: overzoom.offsetY * cropHeight,
            width: cropWidth,
            height: cropHeight
        )
        guard let croppedImage = image.cropping(to: cropRect),
              let colorSpace = image.colorSpace ?? CGColorSpace(name: CGColorSpace.sRGB),
              let context = CGContext(
                  data: nil,
                  width: image.width,
                  height: image.height,
                  bitsPerComponent: 8,
                  bytesPerRow: 0,
                  space: colorSpace,
                  bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
              ) else {
            return transparentNoData
        }

        context.interpolationQuality = .none
        context.draw(
            croppedImage,
            in: CGRect(x: 0, y: 0, width: image.width, height: image.height)
        )

        guard let overzoomedImage = context.makeImage() else {
            return transparentNoData
        }

        return pngData(for: overzoomedImage)
    }

    private static func pngData(for image: CGImage) -> Data {
        let data = NSMutableData()
        guard let destination = CGImageDestinationCreateWithData(
            data,
            pngTypeIdentifier,
            1,
            nil
        ) else {
            return transparentNoData
        }

        CGImageDestinationAddImage(destination, image, nil)
        guard CGImageDestinationFinalize(destination) else {
            return transparentNoData
        }

        return data as Data
    }
}

private actor TileDataCache {
    private var tiles: [URL: Data] = [:]

    func data(for url: URL) -> Data? {
        tiles[url]
    }

    func store(_ data: Data, for url: URL) {
        tiles[url] = data
    }
}
