import Foundation

/// Text translated into the machine's language (`app/Resources/<language>.lproj/Localizable.strings`).
/// Keys are checked by `scripts/check-l10n.py`: a missing key fails the build.
/// Arguments replace `%1$@`, `%2$@`…: always pass `String`s.
// swift-format-ignore: AlwaysUseLowerCamelCase
// One letter on purpose: it wraps every string shown to the user.
func L(_ key: String, _ args: String...) -> String {
    let format = Bundle.main.localizedString(forKey: key, value: nil, table: nil)
    return args.isEmpty ? format : String(format: format, arguments: args)
}

extension Reason {
    var text: String {
        switch self {
        case .connectingToCluster: L("reason.connectingToCluster")
        case .searchingPod: L("reason.searchingPod")
        case .noReadyPod: L("reason.noReadyPod")
        case .podNotReady(let pod): L("reason.podNotReady", pod)
        case .apiUnreachable(let detail): L("reason.apiUnreachable", detail)
        case .apiError(let code, let detail): L("reason.apiError", String(code), detail)
        case .portInUse(let port): L("reason.portInUse", String(port))
        case .portUnavailable(let port, let detail): L("reason.portUnavailable", String(port), detail)
        case .authFailed(let detail): L("reason.authFailed", detail)
        case .accessDenied(let detail): L("reason.accessDenied", detail)
        case .notFound(let detail): L("reason.notFound", detail)
        case .serviceNotFound(let namespace, let service): L("reason.serviceNotFound", namespace, service)
        case .externalNameService: L("reason.externalNameService")
        case .serviceWithoutSelector: L("reason.serviceWithoutSelector")
        case .portNotExposed(let port): L("reason.portNotExposed", String(port))
        case .invalidTargetPort(let value): L("reason.invalidTargetPort", String(value))
        case .contextMissing(let context): L("reason.contextMissing", context)
        case .kubeconfig(let detail): L("reason.kubeconfig", detail)
        }
    }
}

extension Notice {
    var text: String {
        switch self {
        case .rivalDeactivated(let rival, let by): L("notice.rivalDeactivated", rival, by)
        case .unknownForward(let name): L("notice.unknownForward", name)
        case .configNotReloaded(let detail): L("notice.configNotReloaded", detail)
        case .coreStartFailed(let detail): L("notice.coreStartFailed", detail)
        case .idleDeactivated(let name, let minutes): L("notice.idleDeactivated", name, idleDuration(minutes))
        }
    }
}

extension Field {
    var text: String {
        switch self {
        case .name: L("form.name")
        case .context: L("form.context")
        case .namespace: L("form.namespace")
        case .target: L("form.target")
        }
    }
}

/// "15 min", "1 h", "4 h".
func idleDuration(_ minutes: UInt32) -> String {
    minutes % 60 == 0 ? L("idle.hours", String(minutes / 60)) : L("idle.minutes", String(minutes))
}

/// Choices offered for the Idle timeout, in minutes: the usual ones, plus `current`
/// when it was set by hand in the config file (otherwise the menu shows no selection).
func idleChoices(including current: UInt32?) -> [UInt32] {
    let standard: [UInt32] = [15, 60, 240]
    guard let current, !standard.contains(current) else { return standard }
    return (standard + [current]).sorted()
}

/// Label of a default setting: "Never" or a duration.
func idleDefaultLabel(_ minutes: UInt32?) -> String {
    minutes.map(idleDuration) ?? L("idle.never")
}

extension ForwardView {
    /// "Inactive in 12 min" while a deadline is running, otherwise `nil`.
    var idleCountdown: String? {
        guard active, let deadline = idleDeadlineUnix else { return nil }
        let seconds = Double(deadline) - Date().timeIntervalSince1970
        let minutes = UInt32(max(1, (seconds / 60).rounded(.up)))
        return L("idle.countdown", idleDuration(minutes))
    }
}

/// Readable message for a core error.
func describe(_ error: Error) -> String {
    guard let error = error as? CoreError else { return error.localizedDescription }
    switch error {
    case .DuplicateName(let name): return L("error.duplicateName", name)
    case .MissingField(let field): return L("error.missingField", field.text)
    case .PortZero: return L("error.portZero")
    case .ConfigFile(let detail): return L("error.configFile", detail)
    case .Cluster(let reason): return reason.text
    case .ClusterList(let detail): return L("error.clusterList", detail)
    case .Stopped: return L("error.stopped")
    }
}
