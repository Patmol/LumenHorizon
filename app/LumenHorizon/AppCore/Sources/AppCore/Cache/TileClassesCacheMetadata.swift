//
//  TileClassesCacheMetadata.swift
//  LumenHorizon
//
//  Created by Cedric Evrard on 6/20/26.
//

import Foundation

/// Freshness record for cached tile-class legend metadata.
///
/// The legend is keyed by `classificationVersion`; if a manifest reports a
/// different version, cached classes are stale and must be refetched.
public struct TileClassesCacheMetadata: Equatable, Sendable, Codable {
    /// Classification version the cached classes describe.
    public let classificationVersion: String

    /// When the app retrieved these classes.
    public let fetchedAt: Date

    public init(classificationVersion: String, fetchedAt: Date) {
        self.classificationVersion = classificationVersion
        self.fetchedAt = fetchedAt
    }
}

extension TileClassesCacheMetadata {
    /// Builds a freshness record for freshly fetched classes.
    public init(classes: TileClasses, fetchedAt: Date) {
        self.init(
            classificationVersion: classes.classificationVersion,
            fetchedAt: fetchedAt
        )
    }
}

extension TileClassesCacheMetadata {
    /// Whether this cached legend matches the given manifest's classification.
    public func matches(_ manifest: TileManifest) -> Bool {
        classificationVersion == manifest.classificationVersion
    }
}
