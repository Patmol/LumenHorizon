//
//  BackendRequestBuilder.swift
//  LumenHorizon
//
//  Created by Cedric Evrard on 6/20/26.
//

import Foundation

/// Builds concrete `URLRequest`s for `BackendEndpoint`s against a base URL.
///
/// The base URL is the host root (e.g. `https://api.lumenhorizon.app`);
/// this builder appends the `/api/v1` version prefix and endpoint segments.
public struct BackendRequestBuilder: Sendable {
    /// API host root, without the `/api/v1` suffix.
    public let baseURL: URL

    /// Fixed version prefix segments applied to every request.
    private static let versionSegments = ["api", "v1"]

    public init(baseURL: URL) {
        self.baseURL = baseURL
    }

    /// Builds a `GET` `URLRequest` for the given endpoint.
    /// - Throws: `BackendError.transport` if a valid URL cannot be formed.
    public func request(for endpoint: BackendEndpoint) throws -> URLRequest {
        guard var components = URLComponents(
            url: baseURL,
            resolvingAgainstBaseURL: false
        ) else {
            throw BackendError.transport(message: "Invalid base URL: \(baseURL)")
        }

        let segments = Self.versionSegments + endpoint.pathSegments
        let basePath = components.path.hasSuffix("/")
            ? String(components.path.dropLast())
            : components.path
        components.path = basePath + "/" + segments.joined(separator: "/")

        let query = endpoint.queryItems
        components.queryItems = query.isEmpty ? nil : query

        guard let url = components.url else {
            throw BackendError.transport(message: "Could not build URL for \(endpoint)")
        }

        var request = URLRequest(url: url)
        request.httpMethod = "GET"
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        return request
    }
}
