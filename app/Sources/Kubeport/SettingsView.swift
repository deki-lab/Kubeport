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
            Text("Réglages").font(.headline)

            Toggle("Lancer Kubeport à l'ouverture de session", isOn: Binding(
                get: { launchAtLogin },
                set: { model.setLaunchAtLogin($0); launchAtLogin = model.launchAtLogin }
            ))
            if model.launchAtLoginNeedsApproval {
                Text("À autoriser dans Réglages Système > Général > Ouverture.")
                    .font(.caption).foregroundStyle(.secondary)
            }

            VStack(alignment: .leading, spacing: 4) {
                Text("Kubeconfig")
                TextField("Défaut : KUBECONFIG du shell, puis ~/.kube/config", text: $kubeconfig)
                    .textFieldStyle(.roundedBorder)
            }

            if let error {
                Text(error).font(.caption).foregroundStyle(.red)
            }

            HStack {
                Spacer()
                Button("Fermer", action: close).keyboardShortcut(.cancelAction)
                Button("Enregistrer", action: save).keyboardShortcut(.defaultAction)
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
