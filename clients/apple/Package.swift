// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "Yevune",
    platforms: [.macOS(.v14)],
    products: [.executable(name: "Yevune", targets: ["Yevune"])],
    targets: [
        .binaryTarget(name: "YevuneCoreFFIBinary", path: "Packages/YevuneCoreFFI/YevuneCoreFFI.xcframework"),
        .target(
            name: "YevuneCoreFFI",
            dependencies: ["YevuneCoreFFIBinary"],
            path: "Packages/YevuneCoreFFI/Sources/YevuneCoreFFI"
        ),
        .executableTarget(name: "Yevune", dependencies: ["YevuneCoreFFI"], path: "Sources/Yevune"),
        .testTarget(name: "YevuneTests", dependencies: ["Yevune"], path: "Tests/YevuneTests"),
    ]
)
