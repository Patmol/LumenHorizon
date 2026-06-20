//
//  AppRuntimeConfiguration.swift
//  LumenHorizon
//
//  Created by Cedric Evrard on 6/19/26.
//

import AppCore
import Foundation

enum AppRuntimeConfiguration {
    static func make(
        bundle: Bundle = .main,
        processEnvironment: [String: String] = ProcessInfo.processInfo.environment
    ) throws -> AppConfiguration {
        if processEnvironment["XCODE_RUNNING_FOR_PREVIEWS"] == "1" {
            return try AppConfiguration.make(environment: AppEnvironment.preview)
        }

        let environment = try AppEnvironment(
            configurationValue: try requiredString(
                forKey: "LHAppEnvironment",
                in: bundle
            )
        )

        let apiBaseURL = try URL(
            configurationValue: try requiredString(
                forKey: "LHAPIBaseURL",
                in: bundle
            )
        )

        return try AppConfiguration.make(
            environment: environment,
            apiBaseURL: apiBaseURL
        )
    }

    private static func requiredString(
        forKey key: String,
        in bundle: Bundle
    ) throws -> String {
        guard let value = bundle.object(forInfoDictionaryKey: key) as? String else {
            throw AppRuntimeConfigurationError.missingInfoValue(key)
        }

        let trimmedValue = value.trimmingCharacters(in: .whitespacesAndNewlines)

        guard !trimmedValue.isEmpty else {
            throw AppRuntimeConfigurationError.emptyInfoValue(key)
        }

        return trimmedValue
    }
}

enum AppRuntimeConfigurationError: Error, Equatable {
    case missingInfoValue(String)
    case emptyInfoValue(String)
    case invalidURL(String)
}

private extension URL {
    init(configurationValue: String) throws {
        guard let url = URL(string: configurationValue) else {
            throw AppRuntimeConfigurationError.invalidURL(configurationValue)
        }

        self = url
    }
}

