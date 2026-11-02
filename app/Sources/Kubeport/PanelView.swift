import SwiftUI

/// Panneau sous l'icône : Forwards groupés par Contexte puis namespace.
struct PanelView: View {
    let model: CoreModel

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if let error = model.startError {
                Label(error, systemImage: "exclamationmark.triangle.fill")
                    .foregroundStyle(.red)
                    .padding(12)
            } else if model.forwards.isEmpty {
                Text("Aucun Forward.\nOuvre la config pour en ajouter.")
                    .foregroundStyle(.secondary)
                    .padding(12)
            } else {
                ScrollView {
                    VStack(alignment: .leading, spacing: 10) {
                        ForEach(groups, id: \.title) { group in
                            GroupSection(title: group.title, forwards: group.forwards, model: model)
                        }
                    }
                    .padding(12)
                }
                .frame(maxHeight: 420)
            }

            if let notice = model.notice {
                Text(notice)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 12)
                    .padding(.bottom, 8)
            }

            Divider()
            Footer(model: model)
        }
        .frame(width: 340)
    }

    private var groups: [(title: String, forwards: [ForwardView])] {
        var order: [String] = []
        var byGroup: [String: [ForwardView]] = [:]
        for forward in model.forwards {
            let key = "\(forward.context) · \(forward.namespace)"
            if byGroup[key] == nil { order.append(key) }
            byGroup[key, default: []].append(forward)
        }
        return order.sorted().map { ($0, byGroup[$0]!) }
    }
}

private struct GroupSection: View {
    let title: String
    let forwards: [ForwardView]
    let model: CoreModel

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title)
                .font(.caption.weight(.semibold))
                .foregroundStyle(.secondary)
            ForEach(forwards, id: \.name) { forward in
                ForwardRow(forward: forward, model: model)
            }
        }
    }
}

private struct ForwardRow: View {
    let forward: ForwardView
    let model: CoreModel

    var body: some View {
        HStack(alignment: .center, spacing: 8) {
            Circle()
                .fill(dotColor)
                .frame(width: 8, height: 8)
            VStack(alignment: .leading, spacing: 1) {
                Text(forward.name).lineLimit(1)
                Text(subtitle)
                    .font(.caption)
                    .foregroundStyle(subtitleColor)
                    .lineLimit(2)
            }
            Spacer(minLength: 8)
            Toggle("", isOn: Binding(
                get: { forward.active },
                set: { model.setActive(forward, $0) }
            ))
            .toggleStyle(.switch)
            .controlSize(.mini)
            .labelsHidden()
        }
        .help(tooltip)
    }

    private var dotColor: Color {
        guard forward.active, let state = forward.state else { return .secondary.opacity(0.4) }
        switch state {
        case .connected: return .green
        case .reconnecting: return .orange
        case .error: return .red
        }
    }

    private var subtitle: String {
        switch forward.state {
        case .reconnecting(let reason)?: return "Reconnexion : \(reason)"
        case .error(let reason)?: return "Erreur : \(reason)"
        default: return "localhost:\(forward.localPort)"
        }
    }

    private var subtitleColor: Color {
        if case .error? = forward.state { return .red }
        return .secondary
    }

    private var tooltip: String {
        let kind = forward.targetKind == .service ? "svc" : "pod"
        var text = "localhost:\(forward.localPort) → \(kind)/\(forward.targetName):\(forward.remotePort)"
        if case .connected(let pod)? = forward.state { text += "\nPod résolu : \(pod)" }
        return text
    }
}

private struct Footer: View {
    let model: CoreModel

    var body: some View {
        HStack(spacing: 4) {
            Button("Ouvrir la config") { model.openConfig() }
            Button("Recharger") { model.reload() }
            Spacer()
            Button("Tout désactiver") { model.deactivateAll() }
                .disabled(model.globalState == .idle)
            Button {
                NSApplication.shared.terminate(nil)
            } label: {
                Image(systemName: "power")
            }
            .help("Quitter Kubeport")
            .keyboardShortcut("q")
        }
        .buttonStyle(.borderless)
        .controlSize(.small)
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
    }
}
