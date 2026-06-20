//
//  ContentView.swift
//  LumenHorizon
//
//  Created by Cedric Evrard on 6/18/26.
//

import AppCore
import MapKit
import SwiftUI

struct ContentView: View {
    let configuration: AppConfiguration

    @State private var cameraPosition: MapCameraPosition

    init(configuration: AppConfiguration) {
        self.configuration = configuration
        _cameraPosition = State(
            initialValue: .region(configuration.mapDefaults.coordinateRegion)
        )
    }

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                Map(position: $cameraPosition)
                    .mapStyle(.standard)
                    .overlay(alignment: .topLeading) {
                        mapStatusCard
                    }
            }
        }
    }

    private var mapStatusCard: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Map foundation ready")
                .font(.headline)

            Text("Environment: \(configuration.environment.rawValue)")
                .font(.subheadline)

#if DEBUG
            Text(configuration.api.baseURL.absoluteString)
                .font(.caption)
                .foregroundStyle(.secondary)
#endif

            if configuration.usesPreviewFixtures {
                Text("Using preview fixtures")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .padding(12)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
        .padding()
        .accessibilityElement(children: .combine)
    }
}

private extension MapDefaults {
    var coordinateRegion: MKCoordinateRegion {
        MKCoordinateRegion(
            center: CLLocationCoordinate2D(
                latitude: center.latitude,
                longitude: center.longitude
            ),
            span: MKCoordinateSpan(
                latitudeDelta: latitudeDelta,
                longitudeDelta: longitudeDelta
            )
        )
    }
}

#Preview {
    ContentView(
        configuration: try! AppConfiguration.make(environment: .preview)
    )
}
