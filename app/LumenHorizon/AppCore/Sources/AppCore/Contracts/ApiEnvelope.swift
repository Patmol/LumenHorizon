//
//  ApiEnvelope.swift
//  LumenHorizon
//
//  Created by Cedric Evrard on 6/20/26.
//

import Foundation

/// Generic backend response envelope: `{ data, meta, error }`.
///
/// Success responses carry a non-nil `data` and `nil` `error`.
/// Failure responses carry `nil` `data` and a non-nil `error`.
public struct ApiEnvelope<Data: Decodable>: Decodable {
    public let data: Data?
    public let meta: ApiMeta?
    public let error: ApiErrorBody?

    public init(data: Data?, meta: ApiMeta?, error: ApiErrorBody?) {
        self.data = data
        self.meta = meta
        self.error = error
    }
}

/// Envelope metadata present on success and failure responses.
public struct ApiMeta: Decodable, Equatable, Sendable {
    public let requestId: String?
    public let timestamp: String?
    public let nextCursor: String?

    private enum CodingKeys: String, CodingKey {
        case requestId = "request_id"
        case timestamp
        case nextCursor = "next_cursor"
    }

    public init(requestId: String?, timestamp: String?, nextCursor: String? = nil) {
        self.requestId = requestId
        self.timestamp = timestamp
        self.nextCursor = nextCursor
    }
}

/// Backend error payload from the envelope `error` field.
public struct ApiErrorBody: Decodable, Equatable {
    public let code: String
    public let message: String
    public let details: String?

    public init(code: String, message: String, details: String?) {
        self.code = code
        self.message = message
        self.details = details
    }
}
