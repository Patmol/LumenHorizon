// swift-tools-version: 5.9

import PackageDescription

let package = Package(
    name: "AppCore",
    platforms: [
        .iOS(.v17),
        .macOS(.v14),
        .visionOS(.v1),
    ],
    products: [
        .library(
            name: "AppCore",
            targets: ["AppCore"]
        ),
    ],
    targets: [
        .target(name: "AppCore"),
    ]
)
