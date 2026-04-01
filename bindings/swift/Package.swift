// SPDX-License-Identifier: MIT

// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "Karu",
    products: [
        .library(
            name: "Karu",
            targets: ["Karu"]
        ),
    ],
    targets: [
        .systemLibrary(
            name: "CKaru",
            path: "Sources/CKaru",
            pkgConfig: nil,
            providers: []
        ),
        .target(
            name: "Karu",
            dependencies: ["CKaru"],
            path: "Sources/Karu"
        ),
        .testTarget(
            name: "KaruTests",
            dependencies: ["Karu"],
            path: "Tests/KaruTests"
        ),
    ]
)
