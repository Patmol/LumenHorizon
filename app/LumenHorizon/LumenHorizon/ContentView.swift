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

    @State private var viewModel: MapViewModel
    @State private var zoomLevel: Double?

    init(configuration: AppConfiguration, viewModel: MapViewModel? = nil) {
        self.configuration = configuration
        let resolved = viewModel
            ?? MapViewModel(client: BackendClient(api: configuration.api))
        _viewModel = State(initialValue: resolved)
    }

    var body: some View {
        NavigationStack {
            DarkSkyMapView(
                configuration: viewModel.renderableConfiguration,
                opacity: viewModel.opacity,
                initialRegion: configuration.mapDefaults.coordinateRegion,
                onZoomChange: { zoomLevel = $0 }
            )
            .ignoresSafeArea(edges: .bottom)
            .overlay(alignment: .topLeading) { statusCard }
            .overlay(alignment: .bottomLeading) { opacityControl }
            .task { await viewModel.load() }
        }
    }

    // MARK: - Status

    @ViewBuilder
    private var statusCard: some View {
        VStack(alignment: .leading, spacing: 6) {
            switch viewModel.state {
            case .idle, .loading:
                Label {
                    Text("Loading dark-sky tiles…")
                } icon: {
                    ProgressView()
                }
                .font(.subheadline)

            case .ready:
                Text("Dark-sky overlay active")
                    .font(.headline)
                Text("VIIRS radiance-based dark-sky evidence")
                    .font(.caption)
                    .foregroundStyle(.secondary)

            case .empty:
                Text("No tile set published yet")
                    .font(.headline)
                Text("The backend has no dark-sky tiles to show.")
                    .font(.caption)
                    .foregroundStyle(.secondary)

            case .unavailable(let reason):
                Text("Dark-sky tiles unavailable")
                    .font(.headline)
                Text(message(for: reason))
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Button("Retry") {
                    Task { await viewModel.retry() }
                }
                .buttonStyle(.borderedProminent)
                .controlSize(.small)
                .disabled(viewModel.state.isLoading)
            }

            if let zoomLevel {
                Text("Zoom: \(zoomLevel, format: .number.precision(.fractionLength(1)))")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .accessibilityLabel("Map zoom level \(zoomLevel, format: .number.precision(.fractionLength(1)))")
            }

#if DEBUG
            Text(configuration.api.baseURL.absoluteString)
                .font(.caption2)
                .foregroundStyle(.tertiary)
#endif
        }
        .padding(12)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
        .padding()
        .accessibilityElement(children: .combine)
    }

    private func message(for reason: MapOverlayUnavailableReason) -> String {
        switch reason {
        case .offline:
            return "Can't reach the backend. Check your connection and retry."
        case .serverError:
            return "The backend couldn't return tiles right now. Try again."
        case .invalidData:
            return "The tile manifest could not be read."
        }
    }

    // MARK: - Opacity

    @ViewBuilder
    private var opacityControl: some View {
        if viewModel.renderableConfiguration != nil {
            OpacityControl(viewModel: viewModel)
                .padding()
        }
    }
}

/// Slider that binds directly to the view model's overlay opacity.
private struct OpacityControl: View {
    @Bindable var viewModel: MapViewModel

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text("Overlay opacity")
                .font(.caption)
                .foregroundStyle(.secondary)
            Slider(value: $viewModel.opacity, in: 0...1) {
                Text("Overlay opacity")
            }
            .frame(width: 200)
            .accessibilityValue(Text(viewModel.opacity, format: .percent.precision(.fractionLength(0))))
        }
        .padding(12)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
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
    // Inject a view model with a fixture loader so the preview shows a
    // ready overlay state without a live backend.
    let manifestJSON = """
    {
      "tile_set_id": "2026-05-21-radiance-dark-sky-v1-00000000-a1",
      "dataset_date": "2026-05-21",
      "generated_at": "2026-05-21T09:15:00Z",
      "classification_version": "radiance-dark-sky-v1",
      "render_version": "tiles-v1",
      "format": "png",
      "tile_size": 256,
      "min_zoom": 3,
      "max_native_zoom": 10,
      "max_display_zoom": 12,
      "bounds": { "west": -125.0, "south": 24.0, "east": -66.0, "north": 50.0 },
      "tile_url_template": "https://tiles.lumenhorizon.com/tiles/demo/{z}/{x}/{y}.png",
      "tile_count": 12345,
      "checksums": { "manifest_sha256": "hex" }
    }
    """

    let previewViewModel = MapViewModel {
        try JSONDecoder().decode(TileManifest.self, from: Data(manifestJSON.utf8))
    }

    return ContentView(
        configuration: try! AppConfiguration.make(environment: .preview),
        viewModel: previewViewModel
    )
}
