//
//  Package.swift
//  LumenHorizon
//
//  Created by Cedric Evrard on 6/19/26.
//

// swift-tools-version: 6.0

import PackageDescription

let package = Package(
    name: "AppCore",
    platforms: [
        .iOS(.v17),
        .macOS(.v14),
        .visionOS(.v1)
    ],
    products: [
        .library(
            name: "AppCore",
            targets: ["AppCore"]
        )
    ],
    targets: [
        .target(
            name: "AppCore"
        ),
        .testTarget(
            name: "AppCoreTests",
            dependencies: ["AppCore"]
        )
    ]
)
