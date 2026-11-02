import SwiftUI

/// What the form edits.
enum FormMode: Equatable {
    case create
    case edit(ForwardView)
    case duplicate(ForwardView)
}

/// Creating and editing a Forward, with lists filled from the cluster:
/// Context → namespace → Service or Pod → remote port → local port.
struct ForwardForm: View {
    let model: CoreModel
    let mode: FormMode
    let close: () -> Void

    @State private var name = ""
    @State private var nameEdited = false
    @State private var context = ""
    @State private var namespace = ""
    @State private var kind: TargetKind = .service
    @State private var targetName = ""
    @State private var remotePort = ""
    @State private var localPort = ""
    @State private var localEdited = false
    @State private var idleTimeout: IdleTimeoutView = .inherit
    @State private var defaultIdle: UInt32?

    @State private var contexts: [String] = []
    @State private var namespaces: [String] = []
    @State private var targets: [TargetOption] = []
    @State private var namespacesError: String?
    @State private var targetsError: String?
    @State private var error: String?
    @State private var saving = false
    @State private var confirmDelete = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ScreenHeader(title: title, back: close)
            InsetDivider()

            SectionTitle(text: L("form.section.target"))
            SettingRow(label: L("form.context")) {
                menuPicker(L("form.context"), selection: $context, values: contexts)
            }
            SettingRow(label: L("form.namespace")) {
                if namespacesError == nil, !namespaces.isEmpty {
                    menuPicker(L("form.namespace"), selection: $namespace, values: namespaces)
                } else {
                    field(L("form.namespace"), text: $namespace)
                }
            }
            SettingRow(label: L("form.type")) {
                Picker(L("form.type"), selection: $kind) {
                    Text(L("form.service")).tag(TargetKind.service)
                    Text(L("form.pod")).tag(TargetKind.pod)
                }
                .pickerStyle(.segmented)
                .labelsHidden()
                .controlSize(.small)
                .fixedSize()
            }
            SettingRow(label: targetLabel) {
                if !filteredTargets.isEmpty {
                    menuPicker(targetLabel, selection: $targetName, values: filteredTargets.map(\.name))
                } else {
                    field(targetLabel, text: $targetName)
                }
            }
            SettingRow(label: L("form.remotePort")) {
                if !portOptions.isEmpty {
                    Picker(L("form.remotePort"), selection: $remotePort) {
                        ForEach(portOptions, id: \.port) { port in
                            Text(port.name.map { "\(port.port) (\($0))" } ?? "\(port.port)")
                                .tag(String(port.port))
                        }
                    }
                    .labelsHidden()
                    .pickerStyle(.menu)
                    .menuStyle(.borderlessButton)
                    .fixedSize()
                } else {
                    field(L("form.remotePort"), text: $remotePort, width: 80)
                }
            }
            if let hint = namespacesError ?? targetsError {
                Hint(text: hint)
            }

            InsetDivider().padding(.top, 4)
            SectionTitle(text: L("form.section.local"))
            SettingRow(label: L("form.localPort")) {
                field(L("form.localPort"), text: Binding(
                    get: { localPort },
                    set: { localPort = $0; localEdited = true }
                ), width: 80)
            }
            SettingRow(label: L("form.name")) {
                field(L("form.name"), text: Binding(
                    get: { name },
                    set: { name = $0; nameEdited = true }
                ))
            }
            SettingRow(label: L("form.idleTimeout")) {
                Picker(L("form.idleTimeout"), selection: $idleTimeout) {
                    Text(L("idle.inherit", idleDefaultLabel(defaultIdle))).tag(IdleTimeoutView.inherit)
                    Text(L("idle.never")).tag(IdleTimeoutView.never)
                    ForEach(idleChoices, id: \.self) { minutes in
                        Text(idleDuration(minutes)).tag(IdleTimeoutView.minutes(minutes: minutes))
                    }
                }
                .labelsHidden()
                .pickerStyle(.menu)
                .menuStyle(.borderlessButton)
                .fixedSize()
            }
            if let error {
                Hint(text: error, color: .red)
            }

            if case .edit(let forward) = mode, confirmDelete {
                // Separate confirmation line: a double click on Delete cannot delete.
                HStack {
                    Text(L("form.deleteQuestion", forward.name)).font(.callout)
                    Spacer()
                    Button(L("form.cancel")) { confirmDelete = false }
                    Button(L("form.delete"), role: .destructive) { delete(forward) }
                }
                .controlSize(.small)
                .padding(8)
                .background(RoundedRectangle(cornerRadius: 8).fill(Color.red.opacity(0.08)))
                .padding(.horizontal, 6)
                .padding(.top, 4)
            }

            InsetDivider().padding(.top, 4)
            HStack {
                if case .edit = mode, !confirmDelete {
                    Button(L("form.delete"), role: .destructive) { confirmDelete = true }
                        .buttonStyle(.plain)
                        .foregroundStyle(.red)
                }
                Spacer()
                Button(L("form.cancel"), action: close)
                Button(L("form.save"), action: save)
                    .keyboardShortcut(.defaultAction)
                    .disabled(saving || !isComplete)
            }
            .controlSize(.small)
            .padding(.horizontal, 10)
            .padding(.top, 6)
            .padding(.bottom, 4)
        }
        .padding(.vertical, 6)
        .padding(.horizontal, 6)
        .task { await start() }
        .onChange(of: context) { _, value in Task { await loadNamespaces(value) } }
        .onChange(of: namespace) { _, value in Task { await loadTargets(context, value) } }
        .onChange(of: kind) { _, _ in pickDefaultTarget() }
        .onChange(of: targetName) { _, _ in pickDefaultPort(); autofill() }
        .onChange(of: remotePort) { _, _ in autofill() }
    }

    /// Right-aligned menu without a frame, like the Wi-Fi menu controls.
    private func menuPicker(_ label: String, selection: Binding<String>, values: [String]) -> some View {
        Picker(label, selection: selection) {
            ForEach(values, id: \.self) { Text($0).tag($0) }
        }
        .labelsHidden()
        .pickerStyle(.menu)
        .menuStyle(.borderlessButton)
        .fixedSize()
    }

    private func field(_ label: String, text: Binding<String>, width: CGFloat = 150) -> some View {
        TextField(label, text: text)
            .textFieldStyle(.roundedBorder)
            .controlSize(.small)
            .multilineTextAlignment(.trailing)
            .frame(width: width)
            .labelsHidden()
    }

    private var title: String {
        switch mode {
        case .create: L("form.title.create")
        case .edit: L("form.title.edit")
        case .duplicate: L("form.title.duplicate")
        }
    }

    private var targetLabel: String { kind == .service ? L("form.service") : L("form.pod") }

    private var filteredTargets: [TargetOption] { targets.filter { $0.kind == kind } }

    private var portOptions: [PortOption] {
        filteredTargets.first { $0.name == targetName }?.ports ?? []
    }

    private var isComplete: Bool {
        !name.trimmingCharacters(in: .whitespaces).isEmpty && !context.isEmpty
            && !namespace.isEmpty && !targetName.isEmpty
            && UInt16(remotePort) != nil && UInt16(localPort) != nil
    }

    // MARK: Loading

    private func start() async {
        if let source = mode.source {
            name = mode == .edit(source) ? source.name : L("form.copyName", source.name)
            nameEdited = true
            context = source.context
            namespace = source.namespace
            kind = source.targetKind
            targetName = source.targetName
            remotePort = String(source.remotePort)
            localPort = String(source.localPort)
            localEdited = true
            idleTimeout = source.idleTimeout
        }
        defaultIdle = try? await model.defaultIdleTimeout()
        do {
            contexts = try await model.contexts()
            if context.isEmpty { context = contexts.first ?? "" }
        } catch {
            self.error = describe(error)
        }
    }

    private func loadNamespaces(_ context: String) async {
        guard !context.isEmpty else { return }
        do {
            namespaces = try await model.namespaces(context: context)
            namespacesError = nil
            if !namespaces.contains(namespace) {
                namespace = namespaces.contains("default") ? "default" : namespaces.first ?? ""
            }
        } catch {
            // No permission to list namespaces: free text entry.
            namespaces = []
            namespacesError = L("form.namespacesUnavailable", describe(error))
        }
    }

    private func loadTargets(_ context: String, _ namespace: String) async {
        guard !context.isEmpty, !namespace.isEmpty else { return }
        do {
            targets = try await model.targets(context: context, namespace: namespace)
            targetsError = nil
        } catch {
            targets = []
            targetsError = L("form.targetsUnavailable", describe(error))
        }
        pickDefaultTarget()
    }

    private func pickDefaultTarget() {
        if !filteredTargets.contains(where: { $0.name == targetName }) {
            targetName = filteredTargets.first?.name ?? (filteredTargets.isEmpty ? targetName : "")
        }
        pickDefaultPort()
    }

    private func pickDefaultPort() {
        let ports = portOptions.map { String($0.port) }
        if !ports.isEmpty, !ports.contains(remotePort) { remotePort = ports[0] }
    }

    /// Prefilled name `target:port` and local port = remote port, until edited.
    private func autofill() {
        if !nameEdited, !targetName.isEmpty { name = "\(targetName):\(remotePort)" }
        if !localEdited { localPort = remotePort }
    }

    // MARK: Actions

    private func save() {
        guard let remote = UInt16(remotePort), let local = UInt16(localPort) else { return }
        let input = ForwardInput(
            name: name, context: context, namespace: namespace, targetKind: kind,
            targetName: targetName, remotePort: remote, localPort: local, idleTimeout: idleTimeout
        )
        let previous: String? = if case .edit(let f) = mode { f.name } else { nil }
        saving = true
        Task {
            defer { saving = false }
            do {
                try await model.save(previousName: previous, input)
                close()
            } catch {
                self.error = describe(error)
            }
        }
    }

    private func delete(_ forward: ForwardView) {
        Task {
            do {
                try await model.remove(forward.name)
                close()
            } catch {
                self.error = describe(error)
            }
        }
    }
}

private extension FormMode {
    var source: ForwardView? {
        switch self {
        case .create: nil
        case .edit(let f), .duplicate(let f): f
        }
    }
}
