//! States of a Forward: Desired state (the toggle) and Actual state (the tunnel).
//!
//! Causes are typed (`Reason`): the UI translates them into the machine's language.
//! `Display` gives English text for the CLI and the logs.

use std::fmt;

/// Cause of a Reconnecting or Error state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reason {
    // Transient causes (Reconnecting); ApiError is an Error for a client error that a
    // retry cannot fix.
    ConnectingToCluster,
    SearchingPod,
    NoReadyPod,
    PodNotReady { pod: String },
    ApiUnreachable { detail: String },
    ApiError { code: u16, detail: String },
    // Causes that need human action (Error).
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

/// Actual state of an Active Forward.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RealState {
    /// The local port is listening and a Resolved pod receives new connections.
    Connected { pod: String },
    /// Transient cause: the app retries on its own.
    Reconnecting(Reason),
    /// Cause that needs human action: no more retries.
    Error(Reason),
}

/// Public view of a Forward: Desired state and, if Active, Actual state.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ForwardStatus {
    pub active: bool,
    pub real: Option<RealState>,
    /// When the Idle timeout will make this Forward Inactive, if no connection
    /// is opened before then.
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
