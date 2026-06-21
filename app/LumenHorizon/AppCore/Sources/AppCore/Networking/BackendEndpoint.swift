//
//  BackendEndpoint.swift
//  LumenHorizon
//
//  Created by Cedric Evrard on 6/20/26.
//

import Foundation

/// A logical backend endpoint under `{api_base_url}/api/v1`.
///
/// Endpoints describe *what* to request; turning one into a concrete
/// `URLRequest` is the job of `BackendRequestBuilder`.
public enum BackendEndpoint: Equatable, Sendable {
    /// `GET /api/v1/tiles/manifest`
    case latestManifest
    /// `GET /api/v1/tiles/manifest/{tile_set_id}`
    case manifest(tileSetId: String)
    /// `GET /api/v1/tiles/sets?limit=&cursor=`
    case tileSets(limit: Int?, cursor: String?)
    /// `GET /api/v1/tiles/classes`
    case tileClasses

    /// Path segments appended after `/api/v1`, each escaped individually.
    var pathSegments: [String] {
        switch self {
        case .latestManifest:
            return ["tiles", "manifest"]
        case .manifest(let tileSetId):
            return ["tiles", "manifest", tileSetId]
        case .tileSets:
            return ["tiles", "sets"]
        case .tileClasses:
            return ["tiles", "classes"]
        }
    }

    /// Query items for this endpoint, if any. Cursors are opaque and passed
    /// through verbatim — never parsed or interpreted.
    var queryItems: [URLQueryItem] {
        switch self {
        case .tileSets(let limit, let cursor):
            var items: [URLQueryItem] = []
            if let limit { items.append(URLQueryItem(name: "limit", value: String(limit))) }
            if let cursor { items.append(URLQueryItem(name: "cursor", value: cursor)) }
            return items
        case .latestManifest, .manifest, .tileClasses:
            return []
        }
    }
}
