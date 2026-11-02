import SwiftUI

@main
struct KubeportApp: App {
    @State private var model = CoreModel()

    var body: some Scene {
        MenuBarExtra {
            PanelView(model: model)
        } label: {
            Image(nsImage: StatusIcon.image(for: model.globalState))
        }
        .menuBarExtraStyle(.window)
    }
}
