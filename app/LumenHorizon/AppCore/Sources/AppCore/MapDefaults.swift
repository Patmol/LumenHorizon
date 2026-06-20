//
//  MapDefaults.swift
//  LumenHorizon
//
//  Created by Cedric Evrard on 6/19/26.
//

import Foundation

public struct MapDefaults: Equatable, Sendable {
    public let center: GeographicCoordinate
    public let latitudeDelta: Double
    public let longitudeDelta: Double

    public init(
        center: GeographicCoordinate,
        latitudeDelta: Double,
        longitudeDelta: Double
    ) {
        self.center = center
        self.latitudeDelta = latitudeDelta
        self.longitudeDelta = longitudeDelta
    }

    public static let world = MapDefaults(
        center: GeographicCoordinate(latitude: 20, longitude: 0),
        latitudeDelta: 120,
        longitudeDelta: 180
    )
}

public struct GeographicCoordinate: Equatable, Sendable {
    public let latitude: Double
    public let longitude: Double

    public init(latitude: Double, longitude: Double) {
        self.latitude = latitude
        self.longitude = longitude
    }
}
