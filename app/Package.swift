// swift-tools-version:6.0
import PackageDescription

let package = Package(
    name: "Kubeport",
    platforms: [.macOS(.v14)],
    targets: [
        // Built by scripts/build-ffi.sh (Rust core + UniFFI headers).
        .binaryTarget(name: "kubeport_ffiFFI", path: "KubeportFFI.xcframework"),
        .executableTarget(
            name: "Kubeport",
            dependencies: ["kubeport_ffiFFI"],
            path: "Sources/Kubeport",
            linkerSettings: [
                .linkedFramework("Security"),
                .linkedFramework("SystemConfiguration"),
            ]
        ),
    ],
    // UniFFI-generated code is not yet annotated for Swift 6 strict concurrency.
    swiftLanguageModes: [.v5]
)
