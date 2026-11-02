//! États d'un Forward : État voulu (le bouton) et État réel (le tunnel).
//!
//! Les causes sont typées (`Reason`) : l'interface les traduit dans la langue de la
//! machine. `Display` donne un texte anglais pour la CLI et les journaux.

use std::fmt;

/// Cause d'une Reconnexion ou d'une Erreur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reason {
    // Causes passagères (Reconnexion).
    ConnectingToCluster,
    SearchingPod,
    NoReadyPod,
    PodNotReady { pod: String },
    ApiUnreachable { detail: String },
    ApiError { code: u16, detail: String },
    // Causes qui demandent une action humaine (Erreur).
    PortInUse { port: u16 },
    PortUnavailable { port: u16, detail: String },
    AuthFailed { detail: String },
    AccessDenied { detail: String },
    NotFound { detail: String },
    ServiceNotFound { namespace: String, service: String },
    ExternalNameService,
    ServiceWithoutSelector,
    PortNotExposed { port: u16 },
    InvalidTargetPort { value: i32 },
    ContextMissing { context: String },
    Kubeconfig { detail: String },
}

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Reason::ConnectingToCluster => write!(f, "connecting to the cluster"),
            Reason::SearchingPod => write!(f, "looking for a Ready pod"),
            Reason::NoReadyPod => write!(f, "no Ready pod"),
            Reason::PodNotReady { pod } => write!(f, "pod {pod} not found or not Ready"),
            Reason::ApiUnreachable { detail } => write!(f, "API unreachable: {detail}"),
            Reason::ApiError { code, detail } => write!(f, "API error {code}: {detail}"),
            Reason::PortInUse { port } => write!(f, "port {port} already in use"),
            Reason::PortUnavailable { port, detail } => write!(f, "port {port}: {detail}"),
            Reason::AuthFailed { detail } => write!(f, "authentication failed: {detail}"),
            Reason::AccessDenied { detail } => write!(f, "access denied: {detail}"),
            Reason::NotFound { detail } => write!(f, "not found: {detail}"),
            Reason::ServiceNotFound { namespace, service } => {
                write!(f, "Service {namespace}/{service} not found")
            }
            Reason::ExternalNameService => write!(f, "ExternalName Service: no pods behind it"),
            Reason::ServiceWithoutSelector => {
                write!(f, "Service without selector: no pods behind it")
            }
            Reason::PortNotExposed { port } => write!(f, "the Service does not expose port {port}"),
            Reason::InvalidTargetPort { value } => write!(f, "invalid targetPort {value}"),
            Reason::ContextMissing { context } => {
                write!(f, "context {context} not in the kubeconfig")
            }
            Reason::Kubeconfig { detail } => write!(f, "kubeconfig: {detail}"),
        }
    }
}

/// État réel d'un Forward Actif.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RealState {
    /// Le port local écoute et un Pod résolu reçoit les nouvelles connexions.
    Connected { pod: String },
    /// Cause passagère : l'app réessaie seule.
    Reconnecting(Reason),
    /// Cause qui demande une action humaine : plus de nouvelle tentative.
    Error(Reason),
}

/// Vue publique d'un Forward : État voulu et, s'il est Actif, État réel.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ForwardStatus {
    pub active: bool,
    pub real: Option<RealState>,
    /// Moment où le Délai d'inactivité passera ce Forward Inactif, si aucune connexion
    /// n'est ouverte d'ici là.
    pub idle_deadline: Option<std::time::SystemTime>,
}

impl fmt::Display for RealState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RealState::Connected { pod } => write!(f, "Connected → {pod}"),
            RealState::Reconnecting(reason) => write!(f, "Reconnecting: {reason}"),
            RealState::Error(reason) => write!(f, "Error: {reason}"),
        }
    }
}
