//
//  TileManifest.swift
//  LumenHorizon
//
//  Created by Cedric Evrard on 6/20/26.
//

import Foundation

/// A generated tile set's manifest, as returned by
/// `GET /api/v1/tiles/manifest` and `/manifest/{tile_set_id}`.
///
/// Only fields the app consumes are decoded. Unknown additive fields
/// (e.g. `processor_version`, `source_granules`) are tolerated and ignored.
public struct TileManifest: Decodable, Equatable, Sendable {
    public let tileSetId: String
    public let datasetDate: String
    public let generatedAt: String
    public let classificationVersion: String
    public let renderVersion: String
    public let format: String
    public let tileSize: Int
    public let minZoom: Int
    public let maxNativeZoom: Int
    public let maxDisplayZoom: Int
    public let bounds: TileBounds
    public let tileURLTemplate: String
    public let tileCount: Int
    public let checksums: TileChecksums

    private enum CodingKeys: String, CodingKey {
        case tileSetId = "tile_set_id"
        case datasetDate = "dataset_date"
        case generatedAt = "generated_at"
        case classificationVersion = "classification_version"
        case renderVersion = "render_version"
        case format
        case tileSize = "tile_size"
        case minZoom = "min_zoom"
        case maxNativeZoom = "max_native_zoom"
        case maxDisplayZoom = "max_display_zoom"
        case bounds
        case tileURLTemplate = "tile_url_template"
        case tileCount = "tile_count"
        case checksums
    }
}

/// Geographic coverage extent of a tile set.
public struct TileBounds: Decodable, Equatable, Sendable {
    public let west: Double
    public let south: Double
    public let east: Double
    public let north: Double
}

/// Manifest integrity evidence.
public struct TileChecksums: Decodable, Equatable, Sendable {
    public let manifestSHA256: String

    private enum CodingKeys: String, CodingKey {
        case manifestSHA256 = "manifest_sha256"
    }
}
