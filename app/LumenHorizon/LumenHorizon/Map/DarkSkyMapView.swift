//
//  DarkSkyMapView.swift
//  LumenHorizon
//
//  Created by Cedric Evrard on 6/21/26.
//

import AppCore
import MapKit
import SwiftUI

/// SwiftUI host for an `MKMapView` that renders the dark-sky tile overlay.
///
/// Driven by `MapOverlayState.configuration` and an opacity value. The overlay
/// is replaced only when the selected tile set changes; opacity updates the
/// live renderer without rebuilding the overlay.
struct DarkSkyMapView {
    /// The overlay configuration to render, or `nil` to show no overlay.
    let configuration: TileOverlayConfiguration?
    /// Overlay opacity in `0...1`.
    let opacity: Double
    /// Initial visible region (applied once).
    let initialRegion: MKCoordinateRegion
    /// Reports the current web-mercator tile zoom whenever the region changes.
    var onZoomChange: ((Double) -> Void)?

    final class Coordinator: NSObject, MKMapViewDelegate {
        /// Standard web-mercator tile edge length used to derive zoom.
        private static let tilePixelWidth = 256.0

        var overlay: DarkSkyTileOverlay?
        weak var renderer: MKTileOverlayRenderer?
        var opacity: Double = MapViewModel.defaultOpacity
        var didSetInitialRegion = false
        var onZoomChange: ((Double) -> Void)?

        func mapView(
            _ mapView: MKMapView,
            rendererFor overlay: MKOverlay
        ) -> MKOverlayRenderer {
            guard let tileOverlay = overlay as? MKTileOverlay else {
                return MKOverlayRenderer(overlay: overlay)
            }
            let renderer = MKTileOverlayRenderer(tileOverlay: tileOverlay)
            renderer.alpha = CGFloat(opacity)
            self.renderer = renderer
            return renderer
        }

        func mapView(_ mapView: MKMapView, regionDidChangeAnimated animated: Bool) {
            let zoom = tileZoom(of: mapView)
            // Defer so SwiftUI state isn't mutated during a view update
            // (a synchronous setRegion can call this from updateMapView).
            DispatchQueue.main.async { [onZoomChange] in
                onZoomChange?(zoom)
            }
        }

        /// Derives the web-mercator tile zoom from the visible map rect.
        private func tileZoom(of mapView: MKMapView) -> Double {
            let visibleWidth = mapView.visibleMapRect.size.width
            let pointsWidth = Double(mapView.bounds.size.width)
            guard visibleWidth > 0, pointsWidth > 0 else { return 0 }

            let worldWidth = MKMapRect.world.size.width
            return log2(
                worldWidth * pointsWidth / (visibleWidth * Self.tilePixelWidth)
            )
        }
    }

    func makeCoordinator() -> Coordinator {
        let coordinator = Coordinator()
        coordinator.opacity = opacity
        coordinator.onZoomChange = onZoomChange
        return coordinator
    }

    private func makeMapView(context: Context) -> MKMapView {
        let mapView = MKMapView()
        mapView.delegate = context.coordinator
        return mapView
    }

    private func updateMapView(_ mapView: MKMapView, context: Context) {
        let coordinator = context.coordinator

        // Keep the latest reporting callback wired to the coordinator.
        coordinator.onZoomChange = onZoomChange

        if !coordinator.didSetInitialRegion {
            mapView.setRegion(initialRegion, animated: false)
            coordinator.didSetInitialRegion = true
        }

        // Opacity: mutate the live renderer, no overlay rebuild.
        coordinator.opacity = opacity
        coordinator.renderer?.alpha = CGFloat(opacity)
        coordinator.renderer?.setNeedsDisplay()

        // Overlay lifecycle: replace only when the tile set identity changes.
        let newTileSetId = configuration?.tileSetId
        let currentTileSetId = coordinator.overlay?.configuration.tileSetId
        guard newTileSetId != currentTileSetId else { return }

        if let existing = coordinator.overlay {
            mapView.removeOverlay(existing)
            coordinator.overlay = nil
            coordinator.renderer = nil
        }

        if let configuration {
            let overlay = DarkSkyTileOverlay(configuration: configuration)
            applyCameraConstraints(to: mapView, overlay: overlay)
            // Above roads but below labels so place names stay readable.
            mapView.addOverlay(overlay, level: .aboveRoads)
            coordinator.overlay = overlay
        } else {
            clearCameraConstraints(on: mapView)
        }
    }

    private func applyCameraConstraints(
        to mapView: MKMapView,
        overlay: DarkSkyTileOverlay
    ) {
        if let zoomRange = overlay.configuration.cameraZoomRange {
            mapView.setCameraZoomRange(
                MKMapView.CameraZoomRange(
                    minCenterCoordinateDistance: zoomRange.minCenterCoordinateDistance,
                    maxCenterCoordinateDistance: zoomRange.maxCenterCoordinateDistance
                ),
                animated: false
            )
        } else {
            mapView.setCameraZoomRange(nil, animated: false)
        }

        mapView.setCameraBoundary(
            MKMapView.CameraBoundary(mapRect: overlay.boundingMapRect),
            animated: false
        )
    }

    private func clearCameraConstraints(on mapView: MKMapView) {
        mapView.setCameraZoomRange(nil, animated: false)
        mapView.setCameraBoundary(nil, animated: false)
    }
}

#if os(macOS)
extension DarkSkyMapView: NSViewRepresentable {
    func makeNSView(context: Context) -> MKMapView { makeMapView(context: context) }
    func updateNSView(_ nsView: MKMapView, context: Context) {
        updateMapView(nsView, context: context)
    }
}
#else
extension DarkSkyMapView: UIViewRepresentable {
    func makeUIView(context: Context) -> MKMapView { makeMapView(context: context) }
    func updateUIView(_ uiView: MKMapView, context: Context) {
        updateMapView(uiView, context: context)
    }
}
#endif
