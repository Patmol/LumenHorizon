//
//  LumenHorizonApp.swift
//  LumenHorizon
//
//  Created by Cedric Evrard on 6/18/26.
//

import AppCore
import SwiftUI

@main
struct LumenHorizonApp: App {
    private let configuration: AppConfiguration
    
    init() {
        do {
            self.configuration = try AppRuntimeConfiguration.make()
        } catch {
            preconditionFailure("Unable to create app configuration: \(error)")
        }
    }

    var body: some Scene {
        WindowGroup {
            ContentView(configuration: configuration)
        }
#if os(macOS)
        .defaultSize(width: 1100, height: 800)
        .windowResizability(.contentSize)
        .commands {
            CommandGroup(replacing: .help) { /* app-specific help */ }
        }
#endif
#if os(visionOS)
        .windowStyle(.plain)
        .defaultSize(width: 1.2, height: 0.9, depth: 0.1, in: .meters)
#endif
    }
}
