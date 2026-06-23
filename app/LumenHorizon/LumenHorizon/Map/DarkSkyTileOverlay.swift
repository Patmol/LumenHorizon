//
//  DarkSkyTileOverlay.swift
//  LumenHorizon
//
//  Created by Cedric Evrard on 6/21/26.
//

import AppCore
import MapKit

/// An `MKTileOverlay` that renders the backend dark-sky PNG tile set.
///
/// All identity, zoom, bounds, and URL behavior come from a validated
/// `TileOverlayConfiguration`, so this class only bridges that contract to
/// MapKit. Tile URLs are produced by the shared `TileURLTemplate`.
final class DarkSkyTileOverlay: MKTileOverlay {
    private static let transparentNoData = Data()

    /// The configuration this overlay renders. Used to detect stale overlays.
    let configuration: TileOverlayConfiguration

    private let tileDataLoader: any HTTPDataFetching

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
        maximumZ = configuration.maxNativeZoom
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
        let urlString = configuration.template.url(z: path.z, x: path.x, y: path.y)
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
        let urlString = configuration.template.url(z: path.z, x: path.x, y: path.y)
        guard let url = URL(string: urlString), Self.isSupportedTileURL(url) else {
            result(Self.transparentNoData, nil)
            return
        }

        let request = URLRequest(url: url)
        Task { [tileDataLoader] in
            do {
                let (data, response) = try await tileDataLoader.data(for: request)
                result(Self.renderableTileData(data, response: response), nil)
            } catch {
                result(Self.transparentNoData, nil)
            }
        }
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
}
