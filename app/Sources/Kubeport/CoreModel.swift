import AppKit
import Network
import Observation

/// État global affiché par l'icône de la barre de menu.
enum GlobalState {
    case idle    // aucun Forward Actif
    case active  // au moins un Forward Actif, aucun en Erreur
    case error   // au moins un Forward Actif en Erreur
}

/// Façade du cœur Rust pour SwiftUI. Toutes les mutations passent par le thread principal.
@Observable
@MainActor
final class CoreModel {
    private(set) var forwards: [ForwardView] = []
    private(set) var notice: String?
    private(set) var startError: String?

    @ObservationIgnored private var core: Core?
    @ObservationIgnored private var noticeTask: Task<Void, Never>?
    @ObservationIgnored private var wakeObserver: NSObjectProtocol?
    @ObservationIgnored private let pathMonitor = NWPathMonitor()
    @ObservationIgnored private var lastPath: String?

    init() {
        let listener = Listener(
            onChange: { [weak self] forwards in Task { @MainActor in self?.forwards = forwards } },
            onNotice: { [weak self] message in Task { @MainActor in self?.show(message) } }
        )
        let env = ProcessInfo.processInfo.environment
        do {
            // KUBEPORT_CONFIG : config alternative (développement, tests).
            core = try Core(configPath: env["KUBEPORT_CONFIG"], listener: listener)
        } catch {
            startError = error.localizedDescription
        }
        // KUBEPORT_ACTIVATE=a,b : Forwards à activer au lancement (mesures, tests).
        for name in (env["KUBEPORT_ACTIVATE"] ?? "").split(separator: ",") {
            core?.activate(name: String(name))
        }
        watchWakeAndNetwork()
    }

    var globalState: GlobalState {
        let active = forwards.filter(\.active)
        if active.isEmpty { return .idle }
        if active.contains(where: { if case .error = $0.state { true } else { false } }) {
            return .error
        }
        return .active
    }

    var configPath: String? { core?.configPath() }

    func setActive(_ forward: ForwardView, _ active: Bool) {
        if active { core?.activate(name: forward.name) } else { core?.deactivate(name: forward.name) }
    }

    func deactivateAll() { core?.deactivateAll() }
    func reload() { core?.reload() }

    func openConfig() {
        guard let path = configPath else { return }
        NSWorkspace.shared.open(URL(fileURLWithPath: path))
    }

    private func show(_ message: String) {
        notice = message
        noticeTask?.cancel()
        noticeTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(6))
            if !Task.isCancelled { self?.notice = nil }
        }
    }

    /// Au réveil et à chaque changement de réseau, les tunnels sont morts : on relance tout.
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
        // Premier signal : état initial, pas un changement.
        guard let previous = lastPath, previous != signature, signature != "offline" else { return }
        core?.reconnectAll()
    }
}

/// Reçoit les événements du cœur, sur le thread Rust.
private final class Listener: CoreListener, @unchecked Sendable {
    private let onChange: ([ForwardView]) -> Void
    private let onNotice: (String) -> Void

    init(onChange: @escaping ([ForwardView]) -> Void, onNotice: @escaping (String) -> Void) {
        self.onChange = onChange
        self.onNotice = onNotice
    }

    func forwardsChanged(forwards: [ForwardView]) { onChange(forwards) }
    func notice(message: String) { onNotice(message) }
}
