//
//  HTTPDataFetching.swift
//  LumenHorizon
//
//  Created by Cedric Evrard on 6/20/26.
//

import Foundation

/// Minimal seam over `URLSession` so the client can be tested without a network.
public protocol HTTPDataFetching: Sendable {
    func data(for request: URLRequest) async throws -> (Data, URLResponse)
}

extension URLSession: HTTPDataFetching {}
