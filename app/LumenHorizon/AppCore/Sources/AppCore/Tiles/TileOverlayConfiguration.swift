//
//  TileOverlayConfiguration.swift
//  LumenHorizon
//
//  Created by Cedric Evrard on 6/21/26.
//

import Foundation

/// A validated, MapKit-free description of how to render a tile set's overlay.
///
/// Built from a `TileManifest` via a throwing initializer so an invalid or
/// unsupported manifest is rejected at the boundary and never produces a
/// broken tile overlay. The app target converts this into an `MKTileOverlay`;
/// keeping this type free of MapKit keeps it unit-testable without rendering.
public struct TileOverlayConfiguration: Equatable, Sendable {
    /// Overlay identity. Used to decide when a stale overlay must be replaced.
    public let tileSetId: String

    /// Validated `{z}/{x}/{y}` tile URL source.
    public let template: TileURLTemplate

    /// Tile edge length in points, e.g. 256.
    public let tileSize: Int

    /// Minimum tile zoom the backend serves.
    public let minZoom: Int

    /// Maximum zoom for which native tiles are requested.
    public let maxNativeZoom: Int

    /// Maximum useful display zoom (tiles above `maxNativeZoom` are upsampled).
    public let maxDisplayZoom: Int

    /// Geographic coverage extent of the tile set.
    public let bounds: TileBounds

    /// Builds a configuration from a manifest, validating everything the
    /// overlay relies on.
    ///
    /// - Throws: `TileOverlayConfigurationError` when the manifest is
    ///   unsupported or internally inconsistent, or
    ///   `TileURLTemplateError` when the URL template is missing placeholders.
    public init(manifest: TileManifest) throws {
        guard manifest.format.lowercased() == "png" else {
            throw TileOverlayConfigurationError.unsupportedFormat(manifest.format)
        }

        guard manifest.tileSize > 0 else {
            throw TileOverlayConfigurationError.invalidTileSize(manifest.tileSize)
        }

        guard manifest.minZoom >= 0,
              manifest.minZoom <= manifest.maxNativeZoom,
              manifest.maxNativeZoom <= manifest.maxDisplayZoom else {
            throw TileOverlayConfigurationError.invalidZoomRange(
                minZoom: manifest.minZoom,
                maxNativeZoom: manifest.maxNativeZoom,
                maxDisplayZoom: manifest.maxDisplayZoom
            )
        }

        try Self.validate(bounds: manifest.bounds)

        self.tileSetId = manifest.tileSetId
        self.template = try TileURLTemplate(manifest.tileURLTemplate)
        self.tileSize = manifest.tileSize
        self.minZoom = manifest.minZoom
        self.maxNativeZoom = manifest.maxNativeZoom
        self.maxDisplayZoom = manifest.maxDisplayZoom
        self.bounds = manifest.bounds
    }

    private static func validate(bounds: TileBounds) throws {
        guard bounds.west < bounds.east,
              bounds.south < bounds.north,
              (-90.0...90.0).contains(bounds.south),
              (-90.0...90.0).contains(bounds.north),
              (-180.0...180.0).contains(bounds.west),
              (-180.0...180.0).contains(bounds.east) else {
            throw TileOverlayConfigurationError.invalidBounds(bounds)
        }
    }
}

/// Reasons a manifest cannot be turned into a renderable tile overlay.
public enum TileOverlayConfigurationError: Error, Equatable, Sendable {
    /// The manifest `format` is not `png`.
    case unsupportedFormat(String)
    /// `tile_size` was zero or negative.
    case invalidTileSize(Int)
    /// Zoom values were negative or not monotonically increasing.
    case invalidZoomRange(minZoom: Int, maxNativeZoom: Int, maxDisplayZoom: Int)
    /// Coverage bounds were degenerate or out of geographic range.
    case invalidBounds(TileBounds)
}
