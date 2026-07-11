// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "MusicApp",
    platforms: [.macOS(.v14)],
    products: [.executable(name: "MusicApp", targets: ["MusicApp"])],
    targets: [
        .binaryTarget(name: "MusicCoreFFI", path: "Packages/CoreFFI/MusicCoreFFI.xcframework"),
        .target(
            name: "CoreFFI",
            dependencies: ["MusicCoreFFI"],
            path: "Packages/CoreFFI/Sources/CoreFFI"
        ),
        .executableTarget(name: "MusicApp", dependencies: ["CoreFFI"], path: "Sources/MusicApp"),
        .testTarget(name: "MusicAppTests", dependencies: ["MusicApp"], path: "Tests/MusicAppTests"),
    ]
)
