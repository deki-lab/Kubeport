import SwiftUI

@main
struct KubeportApp: App {
    var body: some Scene {
        MenuBarExtra("Kubeport", systemImage: "point.3.connected.trianglepath.dotted") {
            Text("Aucun Forward")
            Divider()
            Button("Quitter") { NSApplication.shared.terminate(nil) }
                .keyboardShortcut("q")
        }
    }
}
