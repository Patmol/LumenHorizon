//
//  BackendError.swift
//  LumenHorizon
//
//  Created by Cedric Evrard on 6/20/26.
//

import Foundation

/// Strongly-typed error surface for backend interactions.
///
/// Distinguishes the three failure layers a caller cares about:
/// - `transport`: the request never produced a usable HTTP response.
/// - `decoding`: the response body did not match the expected contract.
/// - `backend`: the server returned a structured `error` envelope.
public enum BackendError: Error, Equatable, Sendable {
    /// URLSession / URL-level failure (no response, connection error, etc.).
    case transport(message: String)

    /// HTTP status outside the success range, with no decodable error envelope.
    case unexpectedStatus(code: Int)

    /// Response body could not be decoded into the expected type.
    case decoding(message: String)

    /// A success (2xx) envelope decoded but carried no `data` and no `error`.
    case missingData

    /// The backend returned a structured error envelope.
    case backend(code: BackendErrorCode, message: String, details: String?)

    /// Convenience constructor from a decoded `ApiErrorBody`.
    public static func backend(_ body: ApiErrorBody) -> BackendError {
        .backend(
            code: BackendErrorCode(rawValue: body.code),
            message: body.message,
            details: body.details
        )
    }
}

/// Known backend error codes, with an `unknown` fallback for forward compatibility.
public enum BackendErrorCode: Equatable, Sendable {
    case invalidRequest
    case notFound
    case tileUnavailable
    case tileNotFound
    case serviceUnavailable
    case unknown(String)

    public init(rawValue: String) {
        switch rawValue {
        case "invalid_request": self = .invalidRequest
        case "not_found": self = .notFound
        case "tile_unavailable": self = .tileUnavailable
        case "tile_not_found": self = .tileNotFound
        case "service_unavailable": self = .serviceUnavailable
        default: self = .unknown(rawValue)
        }
    }

    /// The wire string for this code (round-trips `unknown`).
    public var rawValue: String {
        switch self {
        case .invalidRequest: return "invalid_request"
        case .notFound: return "not_found"
        case .tileUnavailable: return "tile_unavailable"
        case .tileNotFound: return "tile_not_found"
        case .serviceUnavailable: return "service_unavailable"
        case .unknown(let value): return value
        }
    }
}
