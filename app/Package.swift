// swift-tools-version:6.0
import PackageDescription

let package = Package(
    name: "Kubeport",
    platforms: [.macOS(.v14)],
    targets: [
        // Construit par scripts/build-ffi.sh (cœur Rust + en-têtes UniFFI).
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
    // Le code généré par UniFFI n'est pas encore annoté pour la concurrence stricte de Swift 6.
    swiftLanguageModes: [.v5]
)
