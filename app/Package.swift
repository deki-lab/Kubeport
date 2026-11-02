// swift-tools-version:6.0
import PackageDescription

let package = Package(
    name: "Kubeport",
    platforms: [.macOS(.v14)],
    targets: [
        .executableTarget(
            name: "Kubeport",
            path: "Sources/Kubeport"
        ),
    ]
)
