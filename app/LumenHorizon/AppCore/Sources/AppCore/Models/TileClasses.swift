//
//  TileClasses.swift
//  LumenHorizon
//
//  Created by Cedric Evrard on 6/20/26.
//

import Foundation

/// Legend/classification metadata from `GET /api/v1/tiles/classes`.
public struct TileClasses: Decodable, Equatable, Sendable {
    public let classificationVersion: String
    public let radianceUnits: String
    public let classes: [TileClass]

    private enum CodingKeys: String, CodingKey {
        case classificationVersion = "classification_version"
        case radianceUnits = "radiance_units"
        case classes
    }
}

/// A single classification bucket used for legend color + label.
public struct TileClass: Decodable, Equatable, Sendable {
    public let classNumber: Int
    public let colorHex: String
    public let label: String
    public let minRadiance: Double
    public let maxRadianceExclusive: Double

    private enum CodingKeys: String, CodingKey {
        case classNumber = "class"
        case colorHex = "color_hex"
        case label
        case minRadiance = "min_radiance"
        case maxRadianceExclusive = "max_radiance_exclusive"
    }
}
