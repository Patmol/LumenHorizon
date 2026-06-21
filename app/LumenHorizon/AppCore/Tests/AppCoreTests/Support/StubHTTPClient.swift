//
//  StubHTTPClient.swift
//  LumenHorizon
//
//  Created by Cedric Evrard on 6/20/26.
//

import Foundation
@testable import AppCore

/// Test double for `HTTPDataFetching` that returns canned responses,
/// so client decoding/error-mapping is exercised without a network.
struct StubHTTPClient: HTTPDataFetching {
    enum Outcome: Sendable {
        /// Return a body + HTTP status code.
        case response(body: Data, statusCode: Int)
        /// Throw a transport-level error before producing a response.
        case failure(any Error)
    }

    let outcome: Outcome

    init(json: String, statusCode: Int = 200) {
        self.outcome = .response(body: Data(json.utf8), statusCode: statusCode)
    }

    init(throwing error: any Error) {
        self.outcome = .failure(error)
    }

    func data(for request: URLRequest) async throws -> (Data, URLResponse) {
        switch outcome {
        case .failure(let error):
            throw error
        case .response(let body, let statusCode):
            guard let url = request.url else { throw URLError(.badURL) }
            let response = HTTPURLResponse(
                url: url,
                statusCode: statusCode,
                httpVersion: "HTTP/1.1",
                headerFields: ["Content-Type": "application/json"]
            )!
            return (body, response)
        }
    }
}
