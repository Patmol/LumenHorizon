//
//  AppConfiguration.swift
//  LumenHorizon
//
//  Created by Cedric Evrard on 6/19/26.
//

import Foundation

public enum AppEnvironment: String, Sendable, CaseIterable {
    case local
    case preview
    case release
    
    public init(configurationValue: String) throws {
        let normalizedValue = configurationValue
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased()
        
        guard let environment = AppEnvironment(rawValue: normalizedValue) else {
            throw AppConfigurationError.invalidEnvironment(configurationValue)
        }
        
        self = environment
    }
}

public struct APIConfiguration: Equatable, Sendable {
    public let baseURL: URL
    public let requestTimeout: TimeInterval

    public init(baseURL: URL, requestTimeout: TimeInterval) throws {
        guard baseURL.scheme == "http" || baseURL.scheme == "https" else {
            throw AppConfigurationError.unsupportedURLScheme(baseURL)
        }

        guard let host = baseURL.host(), !host.isEmpty else {
            throw AppConfigurationError.missingHost(baseURL)
        }

        guard requestTimeout > 0 else {
            throw AppConfigurationError.invalidRequestTimeout(requestTimeout)
        }

        self.baseURL = baseURL
        self.requestTimeout = requestTimeout
    }
}

public struct AppConfiguration: Equatable, Sendable {
    public let environment: AppEnvironment
    public let api: APIConfiguration
    public let mapDefaults: MapDefaults
    public let usesPreviewFixtures: Bool

    public init(
        environment: AppEnvironment,
        api: APIConfiguration,
        mapDefaults: MapDefaults = .world,
        usesPreviewFixtures: Bool = false
    ) {
        self.environment = environment
        self.api = api
        self.mapDefaults = mapDefaults
        self.usesPreviewFixtures = usesPreviewFixtures
    }
    
    public static func make(
        environment: AppEnvironment,
        apiBaseURL: URL? = nil,
    ) throws -> AppConfiguration {
        let resolvedAPIBaseURL = apiBaseURL ?? defaultAPIBaseURL(for: environment)
        
        let configuration = try AppConfiguration(
            environment: environment,
            api: APIConfiguration(
                baseURL: resolvedAPIBaseURL,
                requestTimeout: defaultRequestTimeout(for: environment)
            ),
            usesPreviewFixtures: environment == .preview
        )
        
        guard !configuration.isReleasePointingAtLocalBackend else {
            throw AppConfigurationError.releasePointsAtLocalBackend(resolvedAPIBaseURL)
        }
        
        return configuration
    }
    
    private static func defaultAPIBaseURL(for environment: AppEnvironment) -> URL {
        switch environment {
        case .local:
            return URL(string: "http://127.0.0.1:8080")!
        case .preview:
            return URL(string: "https://preview.invalid")!
        case .release:
            return URL(string: "https://api.lumenhorizon.app")!
        }
    }
    
    private static func defaultRequestTimeout(for environment: AppEnvironment) -> TimeInterval {
        switch environment {
        case .preview:
            return 5
        case .local, .release:
            return 30
        }
    }
}

public enum AppConfigurationError: Error, Equatable, Sendable {
    case unsupportedURLScheme(URL)
    case missingHost(URL)
    case invalidRequestTimeout(TimeInterval)
    case invalidEnvironment(String)
    case releasePointsAtLocalBackend(URL)
}

public extension AppConfiguration {
    var isReleasePointingAtLocalBackend: Bool {
        guard environment == .release else {
            return false
        }

        let host = api.baseURL.host()?.lowercased() ?? ""

        return host == "localhost"
            || host == "127.0.0.1"
            || host == "::1"
            || host.hasSuffix(".local")
    }
}
