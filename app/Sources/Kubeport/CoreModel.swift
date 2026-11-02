import AppKit
import Network
import Observation
import ServiceManagement
import UserNotifications

/// Overall state shown by the menu bar icon.
enum GlobalState {
    case idle    // no Active Forward
    case active  // at least one Active Forward, none in Error
    case error   // at least one Active Forward in Error
}

/// Facade of the Rust core for SwiftUI. Every mutation goes through the main thread.
@Observable
@MainActor
final class CoreModel {
    private(set) var forwards: [ForwardView] = []
    private(set) var notice: String?
    private(set) var startError: String?
    /// False until the core sends its first list: avoids a false empty state at launch.
    private(set) var hasLoaded = false
    /// The master switch can turn a remembered set back on (kept by the core).
    private(set) var canRestore = false
    /// "Inactive Forwards" section expanded; remembered between openings.
    var inactiveExpanded: Bool = UserDefaults.standard.bool(forKey: "inactiveExpanded") {
        didSet { UserDefaults.standard.set(inactiveExpanded, forKey: "inactiveExpanded") }
    }

    @ObservationIgnored private var core: Core?
    @ObservationIgnored private var noticeTask: Task<Void, Never>?
    @ObservationIgnored private var wakeObserver: NSObjectProtocol?
    @ObservationIgnored private let pathMonitor = NWPathMonitor()
    @ObservationIgnored private var lastPath: String?
    @ObservationIgnored private var notificationsRequested = false

    init() {
        start()
        // KUBEPORT_ACTIVATE=a,b: Forwards to activate at launch (measurements, tests).
        for name in (ProcessInfo.processInfo.environment["KUBEPORT_ACTIVATE"] ?? "").split(separator: ",") {
            core?.activate(name: String(name))
        }
        watchWakeAndNetwork()
    }

    /// Starts the core; on failure (unreadable config) keeps the error for the panel.
    private func start() {
        let listener = Listener(
            onChange: { [weak self] forwards, canRestore in
                Task { @MainActor in self?.update(forwards, canRestore: canRestore) }
            },
            onNotice: { [weak self] notice in Task { @MainActor in self?.show(notice.text) } }
        )
        do {
            core = try Core(configPath: Self.configOverride, listener: listener)
            startError = nil
        } catch {
            core = nil
            startError = describe(error)
        }
    }

    /// "Try Again" after a start error, once the config file is fixed.
    func restart() {
        hasLoaded = false
        start()
    }

    /// KUBEPORT_CONFIG: alternate config (development, tests).
    private static var configOverride: String? { ProcessInfo.processInfo.environment["KUBEPORT_CONFIG"] }

    var isReady: Bool { core != nil && hasLoaded }

    // MARK: List

    struct Group {
        let title: String
        let forwards: [ForwardView]
    }

    var activeGroups: [Group] { Self.grouped(forwards.filter(\.active)) }
    var inactiveGroups: [Group] { Self.grouped(forwards.filter { !$0.active }) }
    var inactiveCount: Int { forwards.filter { !$0.active }.count }

    /// Groups by "Context · namespace", sorted; config order inside a group.
    private static func grouped(_ forwards: [ForwardView]) -> [Group] {
        var byTitle: [String: [ForwardView]] = [:]
        for forward in forwards {
            byTitle["\(forward.context) · \(forward.namespace)", default: []].append(forward)
        }
        return byTitle.keys.sorted().map { Group(title: $0, forwards: byTitle[$0]!) }
    }

    // MARK: Master switch

    var masterOn: Bool { forwards.contains(where: \.active) }
    var masterEnabled: Bool { masterOn || canRestore }

    /// Off: every Forward off, the set remembered by the core. On: that set back on.
    func setMaster(_ on: Bool) { core?.setAll(on: on) }

    func copyAddress(_ forward: ForwardView) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString("localhost:\(forward.localPort)", forType: .string)
    }

    func dismissNotice() {
        noticeTask?.cancel()
        notice = nil
    }

    var version: String {
        Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "dev"
    }

    var globalState: GlobalState {
        let active = forwards.filter(\.active)
        if active.isEmpty { return .idle }
        if active.contains(where: \.isInError) { return .error }
        return .active
    }

    /// Config file path, known even when the core could not start.
    var configPath: String {
        core?.configPath() ?? Self.configOverride ?? FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent("Library/Application Support/k8s_forwarder/config.toml").path
    }

    // MARK: Commands

    func setActive(_ forward: ForwardView, _ active: Bool) {
        if active { core?.activate(name: forward.name) } else { core?.deactivate(name: forward.name) }
    }

    func reload() { core?.reload() }

    func openConfig() {
        NSWorkspace.shared.open(URL(fileURLWithPath: configPath))
    }

    func quit() { NSApplication.shared.terminate(nil) }

    // MARK: Form and settings

    func contexts() async throws -> [String] { try await requireCore().contexts() }

    func namespaces(context: String) async throws -> [String] {
        try await requireCore().namespaces(context: context)
    }

    func targets(context: String, namespace: String) async throws -> [TargetOption] {
        try await requireCore().targets(context: context, namespace: namespace)
    }

    func save(previousName: String?, _ input: ForwardInput) async throws {
        try await requireCore().saveForward(previousName: previousName, forward: input)
    }

    func remove(_ name: String) async throws { try await requireCore().removeForward(name: name) }

    func kubeconfig() async throws -> String? { try await requireCore().kubeconfig() }

    func defaultIdleTimeout() async throws -> UInt32? { try await requireCore().defaultIdleTimeout() }

    func setDefaultIdleTimeout(_ minutes: UInt32?) async throws {
        try await requireCore().setDefaultIdleTimeout(minutes: minutes)
    }

    func setKubeconfig(_ path: String?) async throws {
        try await requireCore().setKubeconfig(path: path)
    }

    var launchAtLogin: Bool { SMAppService.mainApp.status == .enabled }

    var launchAtLoginNeedsApproval: Bool { SMAppService.mainApp.status == .requiresApproval }

    func setLaunchAtLogin(_ enabled: Bool) {
        do {
            if enabled { try SMAppService.mainApp.register() } else { try SMAppService.mainApp.unregister() }
        } catch {
            show(L("notice.launchAtLogin", error.localizedDescription))
        }
    }

    private func requireCore() throws -> Core {
        guard let core else { throw CoreError.Stopped }
        return core
    }

    // MARK: Core events

    private func update(_ next: [ForwardView], canRestore: Bool) {
        let previous = Dictionary(forwards.map { ($0.name, $0) }, uniquingKeysWith: { a, _ in a })
        forwards = next
        self.canRestore = canRestore
        hasLoaded = true
        for forward in next where forward.isInError && previous[forward.name]?.isInError != true {
            notifyError(forward)
        }
    }

    /// Notify only on entering Error; Reconnecting stays silent.
    private func notifyError(_ forward: ForwardView) {
        guard case .error(let reason)? = forward.state else { return }
        let center = UNUserNotificationCenter.current()
        if !notificationsRequested {
            notificationsRequested = true
            center.requestAuthorization(options: [.alert, .sound]) { _, _ in }
        }
        let content = UNMutableNotificationContent()
        content.title = L("notification.errorTitle", forward.name)
        content.body = reason.text
        center.add(UNNotificationRequest(identifier: "error-\(forward.name)", content: content, trigger: nil))
    }

    func show(_ message: String) {
        notice = message
        noticeTask?.cancel()
        noticeTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(6))
            if !Task.isCancelled { self?.notice = nil }
        }
    }

    /// On wake-up and on every network change, the tunnels are dead: restart everything.
    private func watchWakeAndNetwork() {
        wakeObserver = NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.didWakeNotification, object: nil, queue: .main
        ) { [weak self] _ in
            Task { @MainActor in self?.core?.reconnectAll() }
        }
        pathMonitor.pathUpdateHandler = { [weak self] path in
            let signature = path.status == .satisfied
                ? path.availableInterfaces.map(\.name).joined(separator: ",")
                : "offline"
            Task { @MainActor in self?.networkChanged(signature) }
        }
        pathMonitor.start(queue: .global(qos: .utility))
    }

    private func networkChanged(_ signature: String) {
        defer { lastPath = signature }
        // First signal: initial state, not a change.
        guard let previous = lastPath, previous != signature, signature != "offline" else { return }
        core?.reconnectAll()
    }
}

extension ForwardView {
    var isInError: Bool {
        if case .error? = state { return true }
        return false
    }
}

/// Receives core events, on the Rust thread.
private final class Listener: CoreListener, @unchecked Sendable {
    private let onChange: ([ForwardView], Bool) -> Void
    private let onNotice: (Notice) -> Void

    init(onChange: @escaping ([ForwardView], Bool) -> Void, onNotice: @escaping (Notice) -> Void) {
        self.onChange = onChange
        self.onNotice = onNotice
    }

    func forwardsChanged(forwards: [ForwardView], canRestore: Bool) { onChange(forwards, canRestore) }
    func notice(notice: Notice) { onNotice(notice) }
}
