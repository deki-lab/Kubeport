/// The Forward form's values and cascade rules, without SwiftUI: Context → namespace →
/// Service or Pod → remote port → local port and name. `ForwardForm` only displays it
/// and loads the lists; every rule is here and tested in `KubeportTests`.
struct FormState: Equatable {
    var context = ""
    var namespace = ""
    var kind: TargetKind = .service
    var targetName = ""
    var remotePort = ""
    var idleTimeout: IdleTimeoutView = .inherit
    private(set) var name = ""
    private(set) var localPort = ""
    /// Typed by the user: no longer filled in from the Target.
    private(set) var nameEdited = false
    private(set) var localEdited = false
    /// Targets offered by the cluster for the current namespace (empty: free text).
    private(set) var targets: [TargetOption] = []

    init() {}

    /// Starts from an existing Forward: its values, kept as they are.
    init(source: ForwardView, name: String) {
        self.name = name
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

    mutating func editName(_ value: String) {
        name = value
        nameEdited = true
    }

    mutating func editLocalPort(_ value: String) {
        localPort = value
        localEdited = true
    }

    /// Namespaces listed: keep the current one if listed, else `default`, else the first.
    mutating func namespacesLoaded(_ namespaces: [String]) {
        if !namespaces.contains(namespace) {
            namespace = namespaces.contains("default") ? "default" : namespaces.first ?? ""
        }
    }

    mutating func targetsLoaded(_ targets: [TargetOption]) {
        self.targets = targets
        pickDefaultTarget()
    }

    /// Service ↔ Pod: the Target must exist in the new list.
    mutating func kindChanged() { pickDefaultTarget() }

    mutating func targetChanged() {
        pickDefaultPort()
        autofill()
    }

    mutating func remotePortChanged() { autofill() }

    var filteredTargets: [TargetOption] { targets.filter { $0.kind == kind } }

    var portOptions: [PortOption] {
        filteredTargets.first { $0.name == targetName }?.ports ?? []
    }

    /// Idle timeout minutes to offer, including a value set by hand in the config file.
    var idleChoices: [UInt32] {
        if case .minutes(let minutes) = idleTimeout { return Kubeport.idleChoices(including: minutes) }
        return Kubeport.idleChoices(including: nil)
    }

    /// The Forward to save, or nil while a field is missing. Same rules as the core:
    /// no blank text, no port 0.
    var input: ForwardInput? {
        let text = [name, context, namespace, targetName]
        guard text.allSatisfy({ !$0.trimmingCharacters(in: .whitespaces).isEmpty }),
            let remote = UInt16(remotePort), remote != 0,
            let local = UInt16(localPort), local != 0
        else { return nil }
        return ForwardInput(
            name: name, context: context, namespace: namespace, targetKind: kind,
            targetName: targetName, remotePort: remote, localPort: local, idleTimeout: idleTimeout
        )
    }

    private mutating func pickDefaultTarget() {
        let names = filteredTargets.map(\.name)
        if !names.contains(targetName) {
            // No list: keep what the user typed.
            targetName = names.first ?? (names.isEmpty ? targetName : "")
        }
        pickDefaultPort()
    }

    private mutating func pickDefaultPort() {
        let ports = portOptions.map { String($0.port) }
        if let first = ports.first, !ports.contains(remotePort) { remotePort = first }
    }

    /// Name `target:port` and local port = remote port, until the user edits them.
    private mutating func autofill() {
        if !nameEdited, !targetName.isEmpty { name = "\(targetName):\(remotePort)" }
        if !localEdited { localPort = remotePort }
    }
}
