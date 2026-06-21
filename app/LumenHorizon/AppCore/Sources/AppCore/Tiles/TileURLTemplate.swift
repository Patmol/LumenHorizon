//
//  TileURLTemplate.swift
//  LumenHorizon
//
//  Created by Cedric Evrard on 6/20/26.
//

import Foundation

/// A validated `{z}/{x}/{y}` tile URL template.
///
/// Construction fails fast if any required placeholder is missing, so an
/// invalid manifest is rejected at the boundary rather than producing
/// broken tile requests later.
public struct TileURLTemplate: Equatable, Sendable {
    /// The raw template string, e.g. `https://.../{z}/{x}/{y}.png`.
    public let rawValue: String

    /// Placeholders required by MapKit tile loading.
    public static let requiredPlaceholders = ["{z}", "{x}", "{y}"]

    /// Creates a validated template.
    /// - Throws: `TileURLTemplateError.missingPlaceholders` if any of
    ///   `{z}`, `{x}`, `{y}` is absent.
    public init(_ rawValue: String) throws {
        let missing = Self.requiredPlaceholders.filter { !rawValue.contains($0) }
        guard missing.isEmpty else {
            throw TileURLTemplateError.missingPlaceholders(missing)
        }
        self.rawValue = rawValue
    }

    /// Substitutes the given tile coordinates into the template.
    public func url(z: Int, x: Int, y: Int) -> String {
        rawValue
            .replacingOccurrences(of: "{z}", with: String(z))
            .replacingOccurrences(of: "{x}", with: String(x))
            .replacingOccurrences(of: "{y}", with: String(y))
    }
}

/// Errors produced while validating a tile URL template.
public enum TileURLTemplateError: Error, Equatable, Sendable {
    /// One or more required placeholders were missing from the template.
    case missingPlaceholders([String])
}
