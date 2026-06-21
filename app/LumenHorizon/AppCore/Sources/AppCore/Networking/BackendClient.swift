//
//  BackendClient.swift
//  LumenHorizon
//
//  Created by Cedric Evrard on 6/20/26.
//

import Foundation

/// Typed, validated access to the anonymous backend tile contracts.
public struct BackendClient: Sendable {
    private let requestBuilder: BackendRequestBuilder
    private let session: HTTPDataFetching
    private let requestTimeout: TimeInterval
    private let decoder: JSONDecoder

    /// Creates a client from an app `APIConfiguration`.
    public init(
        api: APIConfiguration,
        session: HTTPDataFetching = URLSession.shared
    ) {
        self.requestBuilder = BackendRequestBuilder(baseURL: api.baseURL)
        self.session = session
        self.requestTimeout = api.requestTimeout
        self.decoder = JSONDecoder()
    }

    // MARK: - Endpoints

    /// `GET /api/v1/tiles/manifest`
    public func latestManifest() async throws -> TileManifest {
        try await fetch(.latestManifest, as: TileManifest.self).data
    }

    /// `GET /api/v1/tiles/manifest/{tile_set_id}`
    public func manifest(tileSetId: String) async throws -> TileManifest {
        try await fetch(.manifest(tileSetId: tileSetId), as: TileManifest.self).data
    }

    /// `GET /api/v1/tiles/sets`. Returns the page plus the opaque
    /// `next_cursor` for the following page (nil when the list is exhausted).
    public func tileSets(
        limit: Int? = nil,
        cursor: String? = nil
    ) async throws -> TileSetPage {
        let result = try await fetch(
            .tileSets(limit: limit, cursor: cursor),
            as: [TileSetSummary].self
        )
        return TileSetPage(sets: result.data, nextCursor: result.meta?.nextCursor)
    }

    /// `GET /api/v1/tiles/classes`
    public func tileClasses() async throws -> TileClasses {
        try await fetch(.tileClasses, as: TileClasses.self).data
    }

    // MARK: - Core request/decode/map

    private func fetch<T: Decodable & Sendable>(
        _ endpoint: BackendEndpoint,
        as type: T.Type
    ) async throws -> (data: T, meta: ApiMeta?) {
        var request = try requestBuilder.request(for: endpoint)
        request.timeoutInterval = requestTimeout

        let data: Data
        let response: URLResponse
        do {
            (data, response) = try await session.data(for: request)
        } catch let error as BackendError {
            throw error
        } catch {
            throw BackendError.transport(message: error.localizedDescription)
        }

        guard let http = response as? HTTPURLResponse else {
            throw BackendError.transport(message: "Non-HTTP response")
        }

        let envelope: ApiEnvelope<T>
        do {
            envelope = try decoder.decode(ApiEnvelope<T>.self, from: data)
        } catch {
            // A non-2xx with an undecodable body is reported by status code.
            if !(200...299).contains(http.statusCode) {
                throw BackendError.unexpectedStatus(code: http.statusCode)
            }
            throw BackendError.decoding(message: String(describing: error))
        }

        if let error = envelope.error {
            throw BackendError.backend(error)
        }

        guard (200...299).contains(http.statusCode) else {
            throw BackendError.unexpectedStatus(code: http.statusCode)
        }

        guard let payload = envelope.data else {
            throw BackendError.missingData
        }

        return (payload, envelope.meta)
    }
}

/// One page of tile-set summaries plus the opaque cursor for the next page.
public struct TileSetPage: Equatable, Sendable {
    public let sets: [TileSetSummary]
    public let nextCursor: String?

    public init(sets: [TileSetSummary], nextCursor: String?) {
        self.sets = sets
        self.nextCursor = nextCursor
    }
}
