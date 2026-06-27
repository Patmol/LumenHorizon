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

        /// Display-zoom band the camera may occupy: `>= 4` and `< 12`.
        /// Coarser levels add little detail and level 12 only upsamples
        /// level-10 data, so both ends are capped here. The half-open `..<`
        /// encodes that 12 itself is never reached.
        private static let reachableZoomRange = 4.0 ..< 12.0

        /// Held back from `reachableZoomRange.upperBound` when constraining the
        /// camera so it stops strictly inside the exclusive ceiling. Without it
        /// the camera could rest exactly on 12.0 at the hard stop and the zoom
        /// readout would momentarily show it. One readout step (the status card
        /// renders one decimal place) is enough.
        private static let upperZoomReadoutMargin = 0.1

        var overlay: DarkSkyTileOverlay?
        weak var renderer: MKTileOverlayRenderer?
        var opacity: Double = MapViewModel.defaultOpacity
        var didSetInitialRegion = false
        var onZoomChange: ((Double) -> Void)?

        /// Display-zoom band (`min_zoom ... max_display_zoom`) of the active
        /// overlay, or `nil` when no overlay constrains the camera.
        private var zoomBand: (minZoom: Int, maxDisplayZoom: Int)?
        /// View width (points) the camera zoom range was last computed for, so
        /// the range is refreshed after a resize but not on every pan/zoom.
        private var appliedZoomRangeWidth: Double?

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
            // The initial layout and any resize land here before the camera
            // zoom range has been computed for the current view size.
            applyCameraZoomRangeIfNeeded(to: mapView)
            let zoom = tileZoom(of: mapView)
            // Defer so SwiftUI state isn't mutated during a view update
            // (a synchronous setRegion can call this from updateMapView).
            DispatchQueue.main.async { [onZoomChange] in
                onZoomChange?(zoom)
            }
        }

        /// Records the overlay's display-zoom band and forces the camera zoom
        /// range to be recomputed on the next layout pass.
        func setZoomBand(minZoom: Int, maxDisplayZoom: Int) {
            if let zoomBand,
               zoomBand.minZoom == minZoom,
               zoomBand.maxDisplayZoom == maxDisplayZoom {
                return
            }
            zoomBand = (minZoom: minZoom, maxDisplayZoom: maxDisplayZoom)
            appliedZoomRangeWidth = nil
        }

        /// Removes any overlay-driven camera zoom range.
        func clearZoomBand(on mapView: MKMapView) {
            zoomBand = nil
            appliedZoomRangeWidth = nil
            mapView.setCameraZoomRange(nil, animated: false)
        }

        /// Constrains the camera to the app's reachable display-zoom band.
        ///
        /// `MKMapView.CameraZoomRange` is expressed in camera-to-center
        /// *distances*, which depend on the view's pixel height and MapKit's
        /// (private) camera field of view. Rather than guess those, this anchors
        /// to the live camera: `centerCoordinateDistance` is proportional to
        /// `2^(-zoom)` for a fixed view, so each limit is the current distance
        /// scaled by its zoom delta. That keeps the constraint consistent with
        /// `tileZoom(of:)` instead of drifting with view size.
        ///
        /// The overlay's advertised band is intersected with
        /// `reachableZoomRange` (`>= 4`, `< 12`), so the camera never zooms out
        /// past level 4 or in to level 12 while still honoring a tile set that
        /// offers a narrower range. `upperZoomReadoutMargin` keeps the zoom-in
        /// limit just below 12 so the readout never reaches 12.0.
        func applyCameraZoomRangeIfNeeded(to mapView: MKMapView) {
            guard let zoomBand else { return }

            let width = Double(mapView.bounds.size.width)
            guard width > 0 else { return }
            if let appliedZoomRangeWidth, abs(appliedZoomRangeWidth - width) < 0.5 {
                return
            }

            let currentZoom = tileZoom(of: mapView)
            let currentDistance = mapView.camera.centerCoordinateDistance
            guard currentZoom > 0, currentDistance > 0, currentDistance.isFinite else {
                return
            }

            let lowerZoom = max(
                Double(zoomBand.minZoom), Self.reachableZoomRange.lowerBound
            )
            let upperZoom = min(
                Double(zoomBand.maxDisplayZoom),
                Self.reachableZoomRange.upperBound - Self.upperZoomReadoutMargin
            )
            guard upperZoom > lowerZoom else { return }

            // Closest distance bounds zoom-in (upperZoom); farthest bounds
            // zoom-out (lowerZoom).
            let minDistance = currentDistance * pow(2, currentZoom - upperZoom)
            let maxDistance = currentDistance * pow(2, currentZoom - lowerZoom)
            guard minDistance > 0, maxDistance >= minDistance else { return }

            mapView.setCameraZoomRange(
                MKMapView.CameraZoomRange(
                    minCenterCoordinateDistance: minDistance,
                    maxCenterCoordinateDistance: maxDistance
                ),
                animated: false
            )
            appliedZoomRangeWidth = width
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

        // Apply a zoom band that was recorded before the view had a size.
        coordinator.applyCameraZoomRangeIfNeeded(to: mapView)

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
            applyCameraConstraints(to: mapView, overlay: overlay, coordinator: coordinator)
            // Above roads but below labels so place names stay readable.
            mapView.addOverlay(overlay, level: .aboveRoads)
            coordinator.overlay = overlay
        } else {
            clearCameraConstraints(on: mapView, coordinator: coordinator)
        }
    }

    private func applyCameraConstraints(
        to mapView: MKMapView,
        overlay: DarkSkyTileOverlay,
        coordinator: Coordinator
    ) {
        // Zoom-band limits depend on the live camera, so the coordinator owns
        // applying and refreshing them; here we just record the band.
        coordinator.setZoomBand(
            minZoom: overlay.configuration.minZoom,
            maxDisplayZoom: overlay.configuration.maxDisplayZoom
        )
        coordinator.applyCameraZoomRangeIfNeeded(to: mapView)

        mapView.setCameraBoundary(
            MKMapView.CameraBoundary(mapRect: overlay.boundingMapRect),
            animated: false
        )
    }

    private func clearCameraConstraints(
        on mapView: MKMapView,
        coordinator: Coordinator
    ) {
        coordinator.clearZoomBand(on: mapView)
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
