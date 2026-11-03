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

    @State private var form = FormState()
    @State private var defaultIdle: UInt32?

    @State private var contexts: [String] = []
    @State private var namespaces: [String] = []
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
                menuPicker(L("form.context"), selection: $form.context, values: contexts)
            }
            SettingRow(label: L("form.namespace")) {
                if namespacesError == nil, !namespaces.isEmpty {
                    menuPicker(L("form.namespace"), selection: $form.namespace, values: namespaces)
                } else {
                    field(L("form.namespace"), text: $form.namespace)
                }
            }
            SettingRow(label: L("form.type")) {
                Picker(L("form.type"), selection: $form.kind) {
                    Text(L("form.service")).tag(TargetKind.service)
                    Text(L("form.pod")).tag(TargetKind.pod)
                }
                .pickerStyle(.segmented)
                .labelsHidden()
                .controlSize(.small)
                .fixedSize()
            }
            SettingRow(label: targetLabel) {
                if !form.filteredTargets.isEmpty {
                    menuPicker(targetLabel, selection: $form.targetName, values: form.filteredTargets.map(\.name))
                } else {
                    field(targetLabel, text: $form.targetName)
                }
            }
            SettingRow(label: L("form.remotePort")) {
                if !form.portOptions.isEmpty {
                    Picker(L("form.remotePort"), selection: $form.remotePort) {
                        ForEach(form.portOptions, id: \.port) { port in
                            Text(port.name.map { "\(port.port) (\($0))" } ?? "\(port.port)")
                                .tag(String(port.port))
                        }
                    }
                    .labelsHidden()
                    .pickerStyle(.menu)
                    .menuStyle(.borderlessButton)
                    .fixedSize()
                } else {
                    field(L("form.remotePort"), text: $form.remotePort, width: 80)
                }
            }
            if let hint = namespacesError ?? targetsError {
                Hint(text: hint)
            }

            InsetDivider().padding(.top, 4)
            SectionTitle(text: L("form.section.local"))
            SettingRow(label: L("form.localPort")) {
                field(
                    L("form.localPort"),
                    text: Binding(
                        get: { form.localPort },
                        set: { form.editLocalPort($0) }
                    ), width: 80)
            }
            SettingRow(label: L("form.name")) {
                field(
                    L("form.name"),
                    text: Binding(
                        get: { form.name },
                        set: { form.editName($0) }
                    ))
            }
            SettingRow(label: L("form.idleTimeout")) {
                Picker(L("form.idleTimeout"), selection: $form.idleTimeout) {
                    Text(L("idle.inherit", idleDefaultLabel(defaultIdle))).tag(IdleTimeoutView.inherit)
                    Text(L("idle.never")).tag(IdleTimeoutView.never)
                    ForEach(form.idleChoices, id: \.self) { minutes in
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
                    .disabled(saving || form.input == nil)
            }
            .controlSize(.small)
            .padding(.horizontal, 10)
            .padding(.top, 6)
            .padding(.bottom, 4)
        }
        .padding(.vertical, 6)
        .padding(.horizontal, 6)
        .task { await start() }
        .onChange(of: form.context) { _, value in Task { await loadNamespaces(value) } }
        .onChange(of: form.namespace) { _, value in Task { await loadTargets(form.context, value) } }
        .onChange(of: form.kind) { _, _ in form.kindChanged() }
        .onChange(of: form.targetName) { _, _ in form.targetChanged() }
        .onChange(of: form.remotePort) { _, _ in form.remotePortChanged() }
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

    private var targetLabel: String { form.kind == .service ? L("form.service") : L("form.pod") }

    // MARK: Loading

    private func start() async {
        if let source = mode.source {
            let name = mode == .edit(source) ? source.name : L("form.copyName", source.name)
            form = FormState(source: source, name: name)
        }
        defaultIdle = try? await model.defaultIdleTimeout()
        do {
            contexts = try await model.contexts()
            if form.context.isEmpty { form.context = contexts.first ?? "" }
        } catch {
            self.error = describe(error)
        }
    }

    private func loadNamespaces(_ context: String) async {
        guard !context.isEmpty else { return }
        do {
            namespaces = try await model.namespaces(context: context)
            namespacesError = nil
            form.namespacesLoaded(namespaces)
        } catch {
            // No permission to list namespaces: free text entry.
            namespaces = []
            namespacesError = L("form.namespacesUnavailable", describe(error))
        }
    }

    private func loadTargets(_ context: String, _ namespace: String) async {
        guard !context.isEmpty, !namespace.isEmpty else { return }
        do {
            form.targetsLoaded(try await model.targets(context: context, namespace: namespace))
            targetsError = nil
        } catch {
            form.targetsLoaded([])
            targetsError = L("form.targetsUnavailable", describe(error))
        }
    }

    // MARK: Actions

    private func save() {
        guard let input = form.input else { return }
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

extension FormMode {
    fileprivate var source: ForwardView? {
        switch self {
        case .create: nil
        case .edit(let f), .duplicate(let f): f
        }
    }
}
