import SwiftUI

/// Réglages : lancement au login et kubeconfig forcé.
struct SettingsView: View {
    let model: CoreModel
    let close: () -> Void

    @State private var launchAtLogin = false
    @State private var kubeconfig = ""
    @State private var error: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(L("settings.title")).font(.headline)

            Toggle(L("settings.launchAtLogin"), isOn: Binding(
                get: { launchAtLogin },
                set: { model.setLaunchAtLogin($0); launchAtLogin = model.launchAtLogin }
            ))
            if model.launchAtLoginNeedsApproval {
                Text(L("settings.needsApproval"))
                    .font(.caption).foregroundStyle(.secondary)
            }

            VStack(alignment: .leading, spacing: 4) {
                Text(L("settings.kubeconfig"))
                TextField(L("settings.kubeconfigPlaceholder"), text: $kubeconfig)
                    .textFieldStyle(.roundedBorder)
            }

            if let error {
                Text(error).font(.caption).foregroundStyle(.red)
            }

            HStack {
                Spacer()
                Button(L("settings.close"), action: close).keyboardShortcut(.cancelAction)
                Button(L("settings.save"), action: save).keyboardShortcut(.defaultAction)
            }
        }
        .padding(12)
        .task {
            launchAtLogin = model.launchAtLogin
            kubeconfig = (try? await model.kubeconfig()) ?? nil ?? ""
        }
    }

    private func save() {
        let trimmed = kubeconfig.trimmingCharacters(in: .whitespaces)
        Task {
            do {
                try await model.setKubeconfig(trimmed.isEmpty ? nil : trimmed)
                close()
            } catch {
                self.error = describe(error)
            }
        }
    }
}
