import SwiftUI

/// Screen shown in the panel: the list, the form or the settings.
enum Screen: Equatable {
    case list
    case form(FormMode)
    case settings
}

/// Panel under the menu bar icon, in the style of the macOS Wi-Fi menu.
struct PanelView: View {
    let model: CoreModel
    @State private var screen: Screen = .list

    var body: some View {
        VStack(spacing: 0) {
            switch screen {
            case .list:
                ListScreen(model: model, navigate: { screen = $0 })
            case .form(let mode):
                ForwardForm(model: model, mode: mode, close: { screen = .list })
            case .settings:
                SettingsView(model: model, close: { screen = .list })
            }
            if let notice = model.notice {
                NoticeBar(text: notice, dismiss: model.dismissNotice)
            }
        }
        .frame(width: 320)
        .background(WindowTopAnchor())
    }
}

// MARK: - List

private struct ListScreen: View {
    let model: CoreModel
    let navigate: (Screen) -> Void
    @State private var expanded: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Header(model: model)
            InsetDivider()
            content
            InsetDivider()
            MenuRow(title: L("footer.new"), shortcut: "n") { navigate(.form(.create)) }
                .disabled(!model.isReady)
            MenuRow(title: L("footer.settings"), shortcut: ",") { navigate(.settings) }
        }
        .padding(.vertical, 6)
        .padding(.horizontal, 6)
    }

    @ViewBuilder private var content: some View {
        if let error = model.startError {
            StartError(message: error, model: model)
        } else if !model.hasLoaded {
            HStack(spacing: 8) {
                ProgressView().controlSize(.small)
                Text(L("panel.loading")).foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, 20)
        } else if model.forwards.isEmpty {
            EmptyState(model: model, create: { navigate(.form(.create)) })
        } else {
            CappedScroll(maxHeight: 440) {
                VStack(alignment: .leading, spacing: 0) {
                    if model.activeGroups.isEmpty {
                        Text(L("panel.noActive"))
                            .font(.callout).foregroundStyle(.secondary)
                            .padding(.horizontal, 10).padding(.vertical, 10)
                    }
                    ForEach(model.activeGroups, id: \.title) { group in
                        SectionTitle(text: group.title)
                        ForEach(group.forwards, id: \.name) { forward in
                            ForwardRow(forward: forward, model: model, expanded: $expanded, navigate: navigate)
                        }
                    }
                    InsetDivider().padding(.top, 4)
                    InactiveDisclosure(model: model)
                    if model.inactiveExpanded {
                        ForEach(model.inactiveGroups, id: \.title) { group in
                            SectionTitle(text: group.title, small: true)
                            ForEach(group.forwards, id: \.name) { forward in
                                ForwardRow(forward: forward, model: model, expanded: $expanded, navigate: navigate)
                            }
                        }
                    }
                }
            }
        }
    }
}

private struct Header: View {
    let model: CoreModel

    var body: some View {
        HStack {
            Text("Kubeport").font(.system(size: 15, weight: .bold))
            Spacer()
            Toggle(L("master.label"), isOn: Binding(get: { model.masterOn }, set: model.setMaster))
                .toggleStyle(.switch)
                .labelsHidden()
                .disabled(!model.masterEnabled)
                .help(L("master.help"))
                .accessibilityLabel(L("master.label"))
        }
        .padding(.horizontal, 10)
        .padding(.top, 4)
        .padding(.bottom, 8)
    }
}

private struct InactiveDisclosure: View {
    let model: CoreModel
    @State private var hovering = false

    var body: some View {
        Button {
            model.inactiveExpanded.toggle()
        } label: {
            HStack {
                Text(L("panel.inactive")).fontWeight(.semibold).foregroundStyle(.secondary)
                Spacer()
                Text(String(model.inactiveCount)).font(.callout).foregroundStyle(.secondary)
                Image(systemName: model.inactiveExpanded ? "chevron.down" : "chevron.right")
                    .font(.caption.weight(.semibold)).foregroundStyle(.secondary)
            }
            .padding(.horizontal, 10).padding(.vertical, 7)
            .contentShape(Rectangle())
            .background(RoundedRectangle(cornerRadius: 8).fill(hovering ? Color.primary.opacity(0.08) : .clear))
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
        .accessibilityValue(model.inactiveExpanded ? L("a11y.expanded") : L("a11y.collapsed"))
    }
}

// MARK: - Row

private struct ForwardRow: View {
    let forward: ForwardView
    let model: CoreModel
    @Binding var expanded: String?
    let navigate: (Screen) -> Void
    @State private var hovering = false

    private var isOpen: Bool { expanded == forward.name }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 10) {
                StateChip(forward: forward)
                VStack(alignment: .leading, spacing: 1) {
                    Text(forward.name)
                        .font(.system(size: forward.active ? 14 : 13))
                        .lineLimit(1).truncationMode(.tail)
                    if let reason = forward.reasonText {
                        Text(reason)
                            .font(.caption)
                            .foregroundStyle(forward.isInError ? Color.red : Color.orange)
                            .lineLimit(2)
                    }
                }
                Spacer(minLength: 6)
                Text(":" + String(forward.localPort))
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundStyle(.secondary)
                Toggle(L("row.switch", forward.name), isOn: Binding(
                    get: { forward.active },
                    set: { model.setActive(forward, $0) }
                ))
                .toggleStyle(.switch)
                .controlSize(.small)
                .labelsHidden()
                .accessibilityLabel(L("row.switch", forward.name))
            }
            .padding(.horizontal, 8)
            .padding(.vertical, forward.active ? 4 : 3)
            .contentShape(Rectangle())
            .onTapGesture { expanded = isOpen ? nil : forward.name }
            .accessibilityAddTraits(.isButton)
            .accessibilityHint(L("row.detailsHint"))

            if isOpen {
                ForwardDetails(forward: forward, model: model, navigate: navigate)
            }
        }
        .background(
            RoundedRectangle(cornerRadius: 8)
                .fill(isOpen || hovering ? Color.primary.opacity(0.08) : .clear)
        )
        .onHover { hovering = $0 }
        .contextMenu {
            Button(L("menu.edit")) { navigate(.form(.edit(forward))) }
            Button(L("menu.duplicate")) { navigate(.form(.duplicate(forward))) }
        }
    }
}

/// Round chip: colour and glyph give the Actual state (shape, not colour alone).
private struct StateChip: View {
    let forward: ForwardView

    var body: some View {
        let size: CGFloat = forward.active ? 28 : 24
        ZStack {
            Circle().fill(fill)
            Image(systemName: glyph)
                .font(.system(size: size * 0.45, weight: .semibold))
                .foregroundStyle(forward.active ? Color.white : Color.primary)
        }
        .frame(width: size, height: size)
        .accessibilityElement()
        .accessibilityLabel(label)
    }

    private var fill: Color {
        guard forward.active else { return Color.primary.opacity(0.1) }
        switch forward.state {
        case .error?: return .red
        case .reconnecting?, nil: return .orange
        case .connected?: return .accentColor
        }
    }

    private var glyph: String {
        switch forward.active ? forward.state : nil {
        case .error?: "exclamationmark"
        case .reconnecting?: "arrow.triangle.2.circlepath"
        default: "arrow.left.arrow.right"
        }
    }

    private var label: String {
        guard forward.active else { return L("state.inactive") }
        switch forward.state {
        case .connected?: return L("state.connected")
        case .error?: return L("state.error")
        default: return L("state.reconnecting")
        }
    }
}

private struct ForwardDetails: View {
    let forward: ForwardView
    let model: CoreModel
    let navigate: (Screen) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Grid(alignment: .leading, horizontalSpacing: 8, verticalSpacing: 4) {
                GridRow {
                    Text(L("detail.target")).foregroundStyle(.secondary)
                    Text(forward.targetLabel).font(.system(size: 11, design: .monospaced)).textSelection(.enabled)
                }
                if case .connected(let pod)? = forward.state {
                    GridRow {
                        Text(L("detail.resolvedPod")).foregroundStyle(.secondary)
                        Text(pod).font(.system(size: 11, design: .monospaced)).textSelection(.enabled)
                    }
                }
                GridRow {
                    Text(L("detail.address")).foregroundStyle(.secondary)
                    HStack(spacing: 6) {
                        Text("localhost:" + String(forward.localPort)).font(.system(size: 11, design: .monospaced))
                        Button(L("detail.copy")) { model.copyAddress(forward) }.controlSize(.mini)
                    }
                }
                if let countdown = forward.idleCountdown {
                    GridRow {
                        Text(L("detail.autoDeactivate")).foregroundStyle(.secondary)
                        Text(countdown)
                    }
                }
            }
            .font(.caption)
            HStack(spacing: 6) {
                Button(L("menu.edit")) { navigate(.form(.edit(forward))) }
                Button(L("menu.duplicate")) { navigate(.form(.duplicate(forward))) }
            }
            .controlSize(.small)
        }
        .padding(.leading, forward.active ? 46 : 42)
        .padding(.trailing, 8)
        .padding(.bottom, 8)
    }
}

// MARK: - Building blocks

/// Grey section title, as in the macOS Wi-Fi menu.
struct SectionTitle: View {
    let text: String
    var small = false

    var body: some View {
        Text(text)
            .font(.system(size: small ? 11 : 12, weight: .semibold))
            .foregroundStyle(.secondary)
            .padding(.horizontal, 10)
            .padding(.top, small ? 6 : 8)
            .padding(.bottom, 3)
            .accessibilityAddTraits(.isHeader)
    }
}

struct InsetDivider: View {
    var body: some View { Divider().padding(.horizontal, 10).padding(.vertical, 2) }
}

/// Plain menu-style row (footer), highlighted on hover like a menu item.
struct MenuRow: View {
    let title: String
    var shortcut: KeyEquivalent?
    var role: ButtonRole?
    let action: () -> Void
    @State private var hovering = false

    var body: some View {
        Button(role: role, action: action) {
            Text(title)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 10).padding(.vertical, 6)
                .contentShape(Rectangle())
                .background(RoundedRectangle(cornerRadius: 8).fill(hovering ? Color.primary.opacity(0.08) : .clear))
        }
        .buttonStyle(.plain)
        .keyboardShortcut(shortcut.map { KeyboardShortcut($0) })
        .onHover { hovering = $0 }
    }
}

/// Scroll area that takes its content's height, up to `maxHeight`, then scrolls.
private struct CappedScroll<Content: View>: View {
    let maxHeight: CGFloat
    @ViewBuilder let content: Content
    @State private var contentHeight: CGFloat = 0

    var body: some View {
        ScrollView {
            content.background(GeometryReader { proxy in
                Color.clear.preference(key: HeightKey.self, value: proxy.size.height)
            })
        }
        .scrollIndicators(contentHeight > maxHeight ? .visible : .hidden)
        .frame(height: min(max(contentHeight, 1), maxHeight))
        .onPreferenceChange(HeightKey.self) { contentHeight = $0 }
    }
}

private struct HeightKey: PreferenceKey {
    static let defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) { value = max(value, nextValue()) }
}

private struct EmptyState: View {
    let model: CoreModel
    let create: () -> Void

    var body: some View {
        VStack(spacing: 8) {
            Image(systemName: "arrow.left.arrow.right.circle")
                .font(.system(size: 28)).foregroundStyle(.secondary)
            Text(L("empty.title")).font(.headline)
            Text(L("empty.body")).font(.callout).foregroundStyle(.secondary).multilineTextAlignment(.center)
            Button(L("empty.create"), action: create).buttonStyle(.borderedProminent)
            Button(L("footer.openConfig"), action: model.openConfig).buttonStyle(.link).font(.callout)
        }
        .frame(maxWidth: .infinity)
        .padding(16)
    }
}

private struct StartError: View {
    let message: String
    let model: CoreModel

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Label(L("startError.title"), systemImage: "exclamationmark.triangle.fill")
                .foregroundStyle(.red).font(.headline)
            Text(message).font(.system(size: 11, design: .monospaced)).textSelection(.enabled)
            HStack {
                Button(L("footer.openConfig"), action: model.openConfig).buttonStyle(.borderedProminent)
                Button(L("startError.retry"), action: model.restart)
            }
            .controlSize(.small)
        }
        .padding(12)
    }
}

private struct NoticeBar: View {
    let text: String
    let dismiss: () -> Void

    var body: some View {
        HStack(alignment: .top, spacing: 6) {
            Image(systemName: "info.circle").foregroundStyle(.secondary)
            Text(text).font(.caption).lineLimit(3).frame(maxWidth: .infinity, alignment: .leading)
            Button(action: dismiss) { Image(systemName: "xmark") }
                .buttonStyle(.plain).foregroundStyle(.secondary)
                .accessibilityLabel(L("notice.dismiss"))
        }
        .padding(10)
        .background(Color.primary.opacity(0.06))
    }
}

extension ForwardView {
    /// Reason shown under the name for Reconnecting and Error.
    var reasonText: String? {
        guard active else { return nil }
        switch state {
        case .reconnecting(let reason)?: return reason.text
        case .error(let reason)?: return reason.text
        default: return nil
        }
    }

    var targetLabel: String {
        (targetKind == .service ? "svc/" : "pod/") + targetName + ":" + String(remotePort)
    }
}
