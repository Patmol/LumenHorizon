//
//  ManifestCacheMetadata.swift
//  LumenHorizon
//
//  Created by Cedric Evrard on 6/20/26.
//

import Foundation

/// Freshness record for a cached "latest" tile manifest.
///
/// Captures the identity and fetch time needed to decide whether a cached
/// manifest is still current. Persistence is handled by a later chunk; this
/// type only models the metadata.
public struct ManifestCacheMetadata: Equatable, Sendable, Codable {
    /// Identity of the cached tile set.
    public let tileSetId: String

    /// Classification version the cached manifest was generated with.
    public let classificationVersion: String

    /// When the app retrieved this manifest.
    public let fetchedAt: Date

    public init(tileSetId: String, classificationVersion: String, fetchedAt: Date) {
        self.tileSetId = tileSetId
        self.classificationVersion = classificationVersion
        self.fetchedAt = fetchedAt
    }
}

extension ManifestCacheMetadata {
    /// Builds a freshness record for a freshly fetched manifest.
    public init(manifest: TileManifest, fetchedAt: Date) {
        self.init(
            tileSetId: manifest.tileSetId,
            classificationVersion: manifest.classificationVersion,
            fetchedAt: fetchedAt
        )
    }
}
