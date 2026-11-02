import SwiftUI

/// Header shared by the form and the settings, in the style of the main panel header:
/// a back chevron and a bold title.
struct ScreenHeader: View {
    let title: String
    let back: () -> Void

    var body: some View {
        HStack(spacing: 6) {
            Button(action: back) {
                Image(systemName: "chevron.left")
                    .font(.system(size: 13, weight: .semibold))
                    .frame(width: 22, height: 22)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
            .keyboardShortcut(.cancelAction)
            .help(L("header.back"))
            .accessibilityLabel(L("header.back"))
            Text(title).font(.system(size: 15, weight: .bold))
            Spacer()
        }
        .padding(.horizontal, 6)
        .padding(.top, 4)
        .padding(.bottom, 8)
    }
}

/// One line: label on the left, control on the right (like the Wi-Fi menu switches).
struct SettingRow<Control: View>: View {
    let label: String
    @ViewBuilder let control: Control

    var body: some View {
        HStack {
            Text(label)
            Spacer(minLength: 8)
            control
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 4)
    }
}

/// Grey explanatory line under a row.
struct Hint: View {
    let text: String
    var color: Color = .secondary

    var body: some View {
        Text(text)
            .font(.caption)
            .foregroundStyle(color)
            .fixedSize(horizontal: false, vertical: true)
            .padding(.horizontal, 10)
            .padding(.bottom, 4)
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
            InsetDivider()

            SectionTitle(text: L("settings.general"))
            SettingRow(label: L("settings.launchAtLogin")) {
                Toggle(L("settings.launchAtLogin"), isOn: Binding(
                    get: { launchAtLogin },
                    set: { model.setLaunchAtLogin($0); launchAtLogin = model.launchAtLogin }
                ))
                .toggleStyle(.switch)
                .controlSize(.small)
                .labelsHidden()
            }
            if model.launchAtLoginNeedsApproval {
                Hint(text: L("settings.needsApproval"))
            }
            SettingRow(label: L("settings.idleTimeout")) {
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
                .labelsHidden()
                .pickerStyle(.menu)
                .menuStyle(.borderlessButton)
                .fixedSize()
            }
            Hint(text: L("settings.idleTimeoutHint"))

            InsetDivider().padding(.top, 4)
            SectionTitle(text: L("settings.kubeconfig"))
            HStack(spacing: 6) {
                TextField(L("settings.kubeconfigPlaceholder"), text: $kubeconfig)
                    .textFieldStyle(.roundedBorder)
                Button(L("settings.choose"), action: choose)
                Button(L("settings.apply"), action: apply)
            }
            .controlSize(.small)
            .padding(.horizontal, 10)
            .padding(.vertical, 4)
            Hint(text: L("settings.kubeconfigHint"))
            if let error {
                Hint(text: error, color: .red)
            }

            InsetDivider().padding(.top, 4)
            MenuRow(title: L("footer.openConfig"), action: model.openConfig)
            MenuRow(title: L("footer.reload"), shortcut: "r", action: model.reload)

            InsetDivider()
            MenuRow(title: L("footer.quit"), shortcut: "q", action: model.quit)
            Text("Kubeport " + model.version)
                .font(.caption)
                .foregroundStyle(.tertiary)
                .padding(.horizontal, 10)
                .padding(.top, 2)
                .padding(.bottom, 4)
        }
        .padding(.vertical, 6)
        .padding(.horizontal, 6)
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
