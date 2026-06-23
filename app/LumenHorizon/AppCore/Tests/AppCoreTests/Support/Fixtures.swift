//
//  Fixtures.swift
//  LumenHorizon
//
//  Created by Cedric Evrard on 6/20/26.
//

import Foundation
@testable import AppCore

/// Backend-aligned JSON fixtures, mirroring the contract reference examples.
enum Fixtures {
    /// A template string lacking the `{x}` and `{y}` placeholders.
    static let templateMissingPlaceholders =
        "https://tiles.lumenhorizon.com/tiles/set/{z}/tile.png"

    static let validTemplate =
        "https://tiles.lumenhorizon.com/tiles/set/{z}/{x}/{y}.png"

    // MARK: Success envelopes

    static let latestManifestEnvelope = """
    {
      "data": \(manifestObject(tileSetId: "2026-05-21-radiance-dark-sky-v1-00000000-a1")),
      "meta": { "request_id": "req-1", "timestamp": "2026-05-21T09:00:00Z" },
      "error": null
    }
    """

    static let manifestByIdEnvelope = """
    {
      "data": \(manifestObject(tileSetId: "2026-04-01-radiance-dark-sky-v1-00000000-b2")),
      "meta": { "request_id": "req-2", "timestamp": "2026-05-21T09:00:00Z" },
      "error": null
    }
    """

    static let tileClassesEnvelope = """
    {
      "data": {
        "classification_version": "radiance-dark-sky-v1",
        "radiance_units": "nW/cm^2/sr",
        "classes": [
          { "class": 1, "color_hex": "#05070d", "label": "Excellent dark site", "min_radiance": 0.0, "max_radiance_exclusive": 0.2 },
          { "class": 2, "color_hex": "#0a1730", "label": "Rural", "min_radiance": 0.2, "max_radiance_exclusive": 1.0 }
        ]
      },
      "meta": { "request_id": "req-3", "timestamp": "2026-05-21T09:00:00Z" },
      "error": null
    }
    """

    static let tileSetsWithCursorEnvelope = """
    {
      "data": [ \(tileSetSummaryObject(tileSetId: "set-a", latest: true)) ],
      "meta": { "request_id": "req-4", "timestamp": "2026-05-21T09:00:00Z", "next_cursor": "opaque-cursor-xyz" },
      "error": null
    }
    """

    static let tileSetsWithoutCursorEnvelope = """
    {
      "data": [ \(tileSetSummaryObject(tileSetId: "set-b", latest: false)) ],
      "meta": { "request_id": "req-5", "timestamp": "2026-05-21T09:00:00Z" },
      "error": null
    }
    """

    // MARK: Failure envelope

    static func errorEnvelope(code: String, message: String) -> String {
        """
        {
          "data": null,
          "meta": { "request_id": "req-err", "timestamp": "2026-05-21T09:00:00Z" },
          "error": { "code": "\(code)", "message": "\(message)", "details": null }
        }
        """
    }

    // MARK: Malformed

    /// Manifest envelope missing the required `tile_url_template` field.
    static let manifestMissingTemplateEnvelope = """
    {
      "data": \(manifestObject(tileSetId: "broken", includeTemplate: false)),
      "meta": { "request_id": "req-6", "timestamp": "2026-05-21T09:00:00Z" },
      "error": null
    }
    """

    // MARK: Builders

    private static func manifestObject(
        tileSetId: String,
        includeTemplate: Bool = true
    ) -> String {
        let templateLine = includeTemplate
            ? "\"tile_url_template\": \"\(validTemplate)\","
            : ""
        return """
        {
          "tile_set_id": "\(tileSetId)",
          "dataset_date": "2026-05-21",
          "generated_at": "2026-05-21T09:15:00Z",
          "classification_version": "radiance-dark-sky-v1",
          "render_version": "tiles-v1",
          "processor_version": "processing-svc:git-sha",
          "format": "png",
          "tile_size": 256,
          "min_zoom": 3,
          "max_native_zoom": 10,
          "max_display_zoom": 12,
          "bounds": { "west": -125.0, "south": 24.0, "east": -66.0, "north": 50.0 },
          \(templateLine)
          "tile_count": 12345,
          "source_granules": [],
          "checksums": { "manifest_sha256": "abc123" }
        }
        """
    }

    private static func tileSetSummaryObject(tileSetId: String, latest: Bool) -> String {
        """
        {
          "tile_set_id": "\(tileSetId)",
          "dataset_date": "2026-05-21",
          "classification_version": "radiance-dark-sky-v1",
          "render_version": "tiles-v1",
          "format": "png",
          "min_zoom": 3,
          "max_native_zoom": 10,
          "max_display_zoom": 12,
          "bounds": { "west": -125.0, "south": 24.0, "east": -66.0, "north": 50.0 },
          "tile_count": 12345,
          "manifest_blob_path": "manifests/\(tileSetId).json",
          "latest": \(latest),
          "created_at": "2026-05-21T09:15:00Z"
        }
        """
    }

    // MARK: Overlay-config builders (Chunk 3)

    /// A standalone manifest JSON object with overridable fields, for
    /// `TileOverlayConfiguration` validation tests.
    static func overlayManifestJSON(
        tileSetId: String = "ts-overlay-1",
        format: String = "png",
        template: String = validTemplate,
        tileSize: Int = 256,
        minZoom: Int = 3,
        maxNativeZoom: Int = 10,
        maxDisplayZoom: Int = 12,
        west: Double = -125.0,
        south: Double = 24.0,
        east: Double = -66.0,
        north: Double = 50.0
    ) -> String {
        """
        {
          "tile_set_id": "\(tileSetId)",
          "dataset_date": "2026-05-21",
          "generated_at": "2026-05-21T09:15:00Z",
          "classification_version": "radiance-dark-sky-v1",
          "render_version": "tiles-v1",
          "format": "\(format)",
          "tile_size": \(tileSize),
          "min_zoom": \(minZoom),
          "max_native_zoom": \(maxNativeZoom),
          "max_display_zoom": \(maxDisplayZoom),
          "bounds": { "west": \(west), "south": \(south), "east": \(east), "north": \(north) },
          "tile_url_template": "\(template)",
          "tile_count": 12345,
          "checksums": { "manifest_sha256": "abc123" }
        }
        """
    }

    /// Decodes an overlay manifest fixture into a `TileManifest`.
    static func overlayManifest(
        tileSetId: String = "ts-overlay-1",
        format: String = "png",
        template: String = validTemplate,
        tileSize: Int = 256,
        minZoom: Int = 3,
        maxNativeZoom: Int = 10,
        maxDisplayZoom: Int = 12,
        west: Double = -125.0,
        south: Double = 24.0,
        east: Double = -66.0,
        north: Double = 50.0
    ) throws -> TileManifest {
        let json = overlayManifestJSON(
            tileSetId: tileSetId, format: format, template: template,
            tileSize: tileSize, minZoom: minZoom, maxNativeZoom: maxNativeZoom,
            maxDisplayZoom: maxDisplayZoom,
            west: west, south: south, east: east, north: north
        )
        return try JSONDecoder().decode(TileManifest.self, from: Data(json.utf8))
    }
}
