//
//  AppConfigurationTests.swift
//  LumenHorizon
//
//  Created by Cedric Evrard on 6/19/26.
//

import AppCore
import Foundation
import Testing

@Suite("App configuration")
struct AppConfigurationTests {
    @Test("local configuration can point at local API Gateway")
    func localConfigurationUsesLocalBackend() throws {
        let configuration = try AppConfiguration.make(environment: .local)

        #expect(configuration.environment == .local)
        #expect(configuration.api.baseURL.absoluteString == "http://127.0.0.1:8080")
        #expect(configuration.usesPreviewFixtures == false)
    }

    @Test("preview configuration uses fixtures")
    func previewConfigurationUsesFixtures() throws {
        let configuration = try AppConfiguration.make(environment: .preview)

        #expect(configuration.environment == .preview)
        #expect(configuration.usesPreviewFixtures == true)
    }

    @Test("release configuration does not point at local backend")
    func releaseConfigurationDoesNotUseLocalBackend() throws {
        let configuration = try AppConfiguration.make(environment: .release)

        #expect(configuration.environment == .release)
        #expect(configuration.isReleasePointingAtLocalBackend == false)
    }

    @Test("API configuration rejects unsupported schemes")
    func apiConfigurationRejectsUnsupportedSchemes() throws {
        let fileURL = try #require(URL(string: "file:///tmp/lumenhorizon"))

        #expect(throws: AppConfigurationError.unsupportedURLScheme(fileURL)) {
            _ = try APIConfiguration(baseURL: fileURL, requestTimeout: 15)
        }
    }

    @Test("API configuration rejects invalid timeout")
    func apiConfigurationRejectsInvalidTimeout() throws {
        let url = try #require(URL(string: "https://api.lumenhorizon.com"))

        #expect(throws: AppConfigurationError.invalidRequestTimeout(0)) {
            _ = try APIConfiguration(baseURL: url, requestTimeout: 0)
        }
    }
    
    @Test("environment parser accepts trimmed case-insensitive values")
    func environmentParserAcceptsBuildSettingValues() throws {
        #expect(try AppEnvironment(configurationValue: " local ") == .local)
        #expect(try AppEnvironment(configurationValue: "PREVIEW") == .preview)
        #expect(try AppEnvironment(configurationValue: "Release") == .release)
    }

    @Test("environment parser rejects unknown values")
    func environmentParserRejectsUnknownValues() throws {
        #expect(throws: AppConfigurationError.invalidEnvironment("staging")) {
            _ = try AppEnvironment(configurationValue: "staging")
        }
    }

    @Test("configuration can use explicit API base URL")
    func configurationCanUseExplicitAPIBaseURL() throws {
        let url = try #require(URL(string: "https://dev-api.lumenhorizon.com"))

        let configuration = try AppConfiguration.make(
            environment: .local,
            apiBaseURL: url
        )

        #expect(configuration.environment == .local)
        #expect(configuration.api.baseURL == url)
    }

    @Test("release configuration rejects local API base URL override")
    func releaseConfigurationRejectsLocalAPIBaseURLOverride() throws {
        let url = try #require(URL(string: "http://127.0.0.1:8080"))

        #expect(throws: AppConfigurationError.releasePointsAtLocalBackend(url)) {
            _ = try AppConfiguration.make(
                environment: .release,
                apiBaseURL: url
            )
        }
    }
}
