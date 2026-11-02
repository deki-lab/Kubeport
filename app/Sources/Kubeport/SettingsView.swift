import SwiftUI

/// Header shared by the form and the settings: "‹ Forwards" back button and a title.
struct ScreenHeader: View {
    let title: String
    let back: () -> Void

    var body: some View {
        HStack {
            Button(action: back) {
                Label(L("header.back"), systemImage: "chevron.left")
            }
            .buttonStyle(.plain)
            .foregroundStyle(Color.accentColor)
            .keyboardShortcut(.cancelAction)
            Spacer()
            Text(title).font(.headline)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
    }
}

/// Settings: launch at login, default Idle timeout, forced kubeconfig, config file, quit.
struct SettingsView: View {
    let model: CoreModel
    let close: () -> Void

    @State private var launchAtLogin = false
    @State private var kubeconfig = ""
    @State private var defaultIdle: UInt32?
    @State private var error: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ScreenHeader(title: L("settings.title"), back: close)
            Divider()
            VStack(alignment: .leading, spacing: 12) {
                Toggle(L("settings.launchAtLogin"), isOn: Binding(
                    get: { launchAtLogin },
                    set: { model.setLaunchAtLogin($0); launchAtLogin = model.launchAtLogin }
                ))
                .toggleStyle(.switch)
                if model.launchAtLoginNeedsApproval {
                    Text(L("settings.needsApproval")).font(.caption).foregroundStyle(.secondary)
                }

                VStack(alignment: .leading, spacing: 4) {
                    Picker(L("settings.idleTimeout"), selection: Binding(
                        get: { defaultIdle },
                        set: { value in
                            defaultIdle = value
                            Task {
                                do { try await model.setDefaultIdleTimeout(value) } catch { self.error = describe(error) }
                            }
                        }
                    )) {
                        Text(L("idle.never")).tag(UInt32?.none)
                        ForEach(idleChoices, id: \.self) { minutes in
                            Text(idleDuration(minutes)).tag(UInt32?.some(minutes))
                        }
                    }
                    Text(L("settings.idleTimeoutHint")).font(.caption).foregroundStyle(.secondary)
                }

                VStack(alignment: .leading, spacing: 4) {
                    Text(L("settings.kubeconfig"))
                    HStack {
                        TextField(L("settings.kubeconfigPlaceholder"), text: $kubeconfig)
                            .textFieldStyle(.roundedBorder)
                        Button(L("settings.choose"), action: choose)
                    }
                    Text(L("settings.kubeconfigHint")).font(.caption).foregroundStyle(.secondary)
                    Button(L("settings.apply"), action: apply).controlSize(.small)
                }

                if let error {
                    Text(error).font(.caption).foregroundStyle(.red)
                }

                HStack {
                    Button(L("footer.openConfig"), action: model.openConfig)
                    Button(L("footer.reload"), action: model.reload).keyboardShortcut("r")
                }
                .controlSize(.small)
            }
            .padding(12)
            Divider()
            HStack {
                Text("Kubeport " + model.version).font(.caption).foregroundStyle(.secondary)
                Spacer()
                Button(L("footer.quit"), action: model.quit).keyboardShortcut("q")
            }
            .controlSize(.small)
            // Keep clear of the panel's rounded bottom corners.
            .padding(.horizontal, 16)
            .padding(.top, 10)
            .padding(.bottom, 14)
        }
        .task {
            launchAtLogin = model.launchAtLogin
            kubeconfig = (try? await model.kubeconfig()) ?? nil ?? ""
            defaultIdle = (try? await model.defaultIdleTimeout()) ?? nil
        }
    }

    private func choose() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.showsHiddenFiles = true
        panel.directoryURL = FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".kube")
        if panel.runModal() == .OK, let url = panel.url { kubeconfig = url.path }
    }

    private func apply() {
        let trimmed = kubeconfig.trimmingCharacters(in: .whitespaces)
        Task {
            do {
                try await model.setKubeconfig(trimmed.isEmpty ? nil : trimmed)
                error = nil
                model.show(L("settings.applied"))
            } catch {
                self.error = describe(error)
            }
        }
    }
}
