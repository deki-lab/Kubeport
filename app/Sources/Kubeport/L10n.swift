import Foundation

/// Texte traduit dans la langue de la machine (`app/Resources/<langue>.lproj/Localizable.strings`).
/// Les clés sont vérifiées par `scripts/check-l10n.py` : une clé absente fait échouer le build.
/// Les arguments remplacent `%1$@`, `%2$@`… : on passe toujours des `String`.
func L(_ key: String, _ args: String...) -> String {
    let format = Bundle.main.localizedString(forKey: key, value: nil, table: nil)
    return args.isEmpty ? format : String(format: format, arguments: args)
}

extension ReasonView {
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
        }
    }
}

extension FormField {
    var text: String {
        switch self {
        case .name: L("form.name")
        case .context: L("form.context")
        case .namespace: L("form.namespace")
        case .target: L("form.target")
        }
    }
}

/// Message lisible pour une erreur du cœur.
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
