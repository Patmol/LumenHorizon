//
//  TileSetSummary.swift
//  LumenHorizon
//
//  Created by Cedric Evrard on 6/20/26.
//

import Foundation

/// A tile-set list item from `GET /api/v1/tiles/sets`, used for
/// historical tile-set selection. Mirrors the backend summary shape,
/// which omits `tile_url_template`/`checksums` (fetch the full manifest
/// by id for those).
public struct TileSetSummary: Decodable, Equatable, Sendable {
    public let tileSetId: String
    public let datasetDate: String
    public let classificationVersion: String
    public let renderVersion: String
    public let format: String
    public let minZoom: Int
    public let maxNativeZoom: Int
    public let maxDisplayZoom: Int
    public let bounds: TileBounds
    public let tileCount: Int
    public let manifestBlobPath: String
    public let latest: Bool
    public let createdAt: String

    private enum CodingKeys: String, CodingKey {
        case tileSetId = "tile_set_id"
        case datasetDate = "dataset_date"
        case classificationVersion = "classification_version"
        case renderVersion = "render_version"
        case format
        case minZoom = "min_zoom"
        case maxNativeZoom = "max_native_zoom"
        case maxDisplayZoom = "max_display_zoom"
        case bounds
        case tileCount = "tile_count"
        case manifestBlobPath = "manifest_blob_path"
        case latest
        case createdAt = "created_at"
    }
}
