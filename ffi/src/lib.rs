//! UniFFI bridge between `kubeport-core` and the SwiftUI app.
//!
//! Swift only sees Forwards, their Desired state and their Actual state (ADR-0003).
//! The core runs on its own thread with a `current_thread` tokio runtime, around a
//! `Session` that owns every rule; this crate only translates calls, views and notices.
//! Swift sends commands and receives changes through `CoreListener`.
//! Form requests (namespaces, Targets, saving) reply through a `oneshot`,
//! awaited on the Swift side as an `async` function.

use std::path::PathBuf;
use std::sync::Arc;

use kubeport_core::catalog::{self, TargetChoice};
use kubeport_core::config::minutes;
use kubeport_core::{
    Config, ConfigError, Field, Forward, IdleTimeout, KubeEnv, RealState, Reason, Session,
    Statuses, Target, login_shell_env,
};
use tokio::sync::{mpsc, oneshot};

uniffi::setup_scaffolding!();

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum TargetKind {
    Service,
    Pod,
}

/// Actual state of an Active Forward.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum RealStateView {
    Connected { pod: String },
    Reconnecting { reason: ReasonView },
    Error { reason: ReasonView },
}

/// Cause of a Reconnecting or Error state, translated on the Swift side.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum ReasonView {
    ConnectingToCluster,
    SearchingPod,
    NoReadyPod,
    PodNotReady { pod: String },
    ApiUnreachable { detail: String },
    ApiError { code: u16, detail: String },
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

impl From<Reason> for ReasonView {
    fn from(reason: Reason) -> Self {
        match reason {
            Reason::ConnectingToCluster => Self::ConnectingToCluster,
            Reason::SearchingPod => Self::SearchingPod,
            Reason::NoReadyPod => Self::NoReadyPod,
            Reason::PodNotReady { pod } => Self::PodNotReady { pod },
            Reason::ApiUnreachable { detail } => Self::ApiUnreachable { detail },
            Reason::ApiError { code, detail } => Self::ApiError { code, detail },
            Reason::PortInUse { port } => Self::PortInUse { port },
            Reason::PortUnavailable { port, detail } => Self::PortUnavailable { port, detail },
            Reason::AuthFailed { detail } => Self::AuthFailed { detail },
            Reason::AccessDenied { detail } => Self::AccessDenied { detail },
            Reason::NotFound { detail } => Self::NotFound { detail },
            Reason::ServiceNotFound { namespace, service } => {
                Self::ServiceNotFound { namespace, service }
            }
            Reason::ExternalNameService => Self::ExternalNameService,
            Reason::ServiceWithoutSelector => Self::ServiceWithoutSelector,
            Reason::PortNotExposed { port } => Self::PortNotExposed { port },
            Reason::InvalidTargetPort { value } => Self::InvalidTargetPort { value },
            Reason::ContextMissing { context } => Self::ContextMissing { context },
            Reason::Kubeconfig { detail } => Self::Kubeconfig { detail },
        }
    }
}

impl std::fmt::Display for ReasonView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

/// Required field missing from the form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FormField {
    Name,
    Context,
    Namespace,
    Target,
}

/// Short message for the user, translated on the Swift side.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum Notice {
    /// `rival` was deactivated: `by` uses the same local port.
    RivalDeactivated {
        rival: String,
        by: String,
    },
    UnknownForward {
        name: String,
    },
    ConfigNotReloaded {
        detail: String,
    },
    CoreStartFailed {
        detail: String,
    },
    /// Idle timeout elapsed: `name` went Inactive.
    IdleDeactivated {
        name: String,
        minutes: u32,
    },
}

impl From<kubeport_core::Notice> for Notice {
    fn from(notice: kubeport_core::Notice) -> Self {
        use kubeport_core::Notice as N;
        match notice {
            N::RivalDeactivated { rival, by } => Self::RivalDeactivated { rival, by },
            N::IdleDeactivated { name, minutes } => Self::IdleDeactivated { name, minutes },
            N::UnknownForward { name } => Self::UnknownForward { name },
            N::ConfigNotReloaded { detail } => Self::ConfigNotReloaded { detail },
        }
    }
}

/// Idle timeout chosen for a Forward.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum IdleTimeoutView {
    /// Follows the default setting.
    Inherit,
    Never,
    Minutes {
        minutes: u32,
    },
}

impl From<IdleTimeout> for IdleTimeoutView {
    fn from(idle: IdleTimeout) -> Self {
        match idle {
            IdleTimeout::Inherit => Self::Inherit,
            IdleTimeout::Never => Self::Never,
            IdleTimeout::After(d) => Self::Minutes {
                minutes: minutes(d),
            },
        }
    }
}

impl From<IdleTimeoutView> for IdleTimeout {
    fn from(idle: IdleTimeoutView) -> Self {
        match idle {
            IdleTimeoutView::Inherit => Self::Inherit,
            IdleTimeoutView::Never => Self::Never,
            IdleTimeoutView::Minutes { minutes } => Self::from_minutes(minutes),
        }
    }
}

/// A Forward as the UI displays it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ForwardView {
    pub name: String,
    pub context: String,
    pub namespace: String,
    pub target_kind: TargetKind,
    pub target_name: String,
    pub remote_port: u16,
    pub local_port: u16,
    /// Desired state.
    pub active: bool,
    /// Actual state, present only if the Forward is Active.
    pub state: Option<RealStateView>,
    /// Idle timeout chosen for this Forward.
    pub idle_timeout: IdleTimeoutView,
    /// Automatic deactivation deadline, in Unix seconds; absent if a connection
    /// is open or there is no timeout.
    pub idle_deadline_unix: Option<i64>,
}

/// A Forward entered in the form.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ForwardInput {
    pub name: String,
    pub context: String,
    pub namespace: String,
    pub target_kind: TargetKind,
    pub target_name: String,
    pub remote_port: u16,
    pub local_port: u16,
    pub idle_timeout: IdleTimeoutView,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PortOption {
    pub port: u16,
    pub name: Option<String>,
}

/// Target offered by the cluster to the form.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TargetOption {
    pub kind: TargetKind,
    pub name: String,
    pub ports: Vec<PortOption>,
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum CoreError {
    #[error("a Forward named {name:?} already exists")]
    DuplicateName { name: String },
    #[error("{field:?} is missing")]
    MissingField { field: FormField },
    #[error("port 0 is not allowed")]
    PortZero,
    #[error("config file: {detail}")]
    ConfigFile { detail: String },
    #[error("{reason}")]
    Cluster { reason: ReasonView },
    #[error("cluster listing failed: {detail}")]
    ClusterList { detail: String },
    #[error("core stopped")]
    Stopped,
}

/// Implemented on the Swift side. Called from the core thread, never the main thread.
#[uniffi::export(callback_interface)]
pub trait CoreListener: Send + Sync {
    /// Full list of Forwards, in config order, on every change. `can_restore`: the master
    /// switch can turn a remembered set back on.
    fn forwards_changed(&self, forwards: Vec<ForwardView>, can_restore: bool);
    /// Short message for the user (Competing Forward deactivated, invalid config…).
    fn notice(&self, notice: Notice);
}

type Reply<T> = oneshot::Sender<Result<T, CoreError>>;

enum Command {
    Activate(String),
    Deactivate(String),
    SetAll(bool),
    ReconnectAll,
    Reload,
    Contexts(Reply<Vec<String>>),
    Namespaces(String, Reply<Vec<String>>),
    Targets(String, String, Reply<Vec<TargetOption>>),
    Save(Option<String>, ForwardInput, Reply<()>),
    Remove(String, Reply<()>),
    SetKubeconfig(Option<String>, Reply<()>),
    Kubeconfig(Reply<Option<String>>),
    DefaultIdle(Reply<Option<u32>>),
    SetDefaultIdle(Option<u32>, Reply<()>),
}

#[derive(uniffi::Object)]
pub struct Core {
    commands: mpsc::UnboundedSender<Command>,
    config_path: String,
}

#[uniffi::export]
impl Core {
    /// Starts the core. `config_path` absent: default location.
    /// A missing file is created with a commented example. Forwards Active at the
    /// last shutdown are reactivated.
    #[uniffi::constructor]
    pub fn new(
        config_path: Option<String>,
        listener: Box<dyn CoreListener>,
    ) -> Result<Arc<Self>, CoreError> {
        let path = config_path
            .map(PathBuf::from)
            .unwrap_or_else(Config::default_path);
        let config = Config::load_or_create(&path).map_err(config_error)?;

        let (commands, receiver) = mpsc::unbounded_channel();
        let thread_path = path.clone();
        std::thread::Builder::new()
            .name("kubeport-core".into())
            .spawn(move || run(config, thread_path, receiver, listener))
            .map_err(|e| CoreError::ConfigFile {
                detail: format!("core thread: {e}"),
            })?;

        Ok(Arc::new(Self {
            commands,
            config_path: path.to_string_lossy().into_owned(),
        }))
    }

    pub fn config_path(&self) -> String {
        self.config_path.clone()
    }

    pub fn activate(&self, name: String) {
        let _ = self.commands.send(Command::Activate(name));
    }

    pub fn deactivate(&self, name: String) {
        let _ = self.commands.send(Command::Deactivate(name));
    }

    /// Master switch. Off: every Forward off, the set remembered. On: that set back on.
    pub fn set_all(&self, on: bool) {
        let _ = self.commands.send(Command::SetAll(on));
    }

    /// Call on Mac wake-up and on every network change.
    pub fn reconnect_all(&self) {
        let _ = self.commands.send(Command::ReconnectAll);
    }

    /// Rereads the config file after a manual edit.
    pub fn reload(&self) {
        let _ = self.commands.send(Command::Reload);
    }

    pub async fn contexts(&self) -> Result<Vec<String>, CoreError> {
        self.ask(Command::Contexts).await
    }

    pub async fn namespaces(&self, context: String) -> Result<Vec<String>, CoreError> {
        self.ask(|reply| Command::Namespaces(context, reply)).await
    }

    pub async fn targets(
        &self,
        context: String,
        namespace: String,
    ) -> Result<Vec<TargetOption>, CoreError> {
        self.ask(|reply| Command::Targets(context, namespace, reply))
            .await
    }

    /// Adds a Forward, or replaces `previous_name`. A modified Active Forward is restarted.
    pub async fn save_forward(
        &self,
        previous_name: Option<String>,
        forward: ForwardInput,
    ) -> Result<(), CoreError> {
        self.ask(|reply| Command::Save(previous_name, forward, reply))
            .await
    }

    pub async fn remove_forward(&self, name: String) -> Result<(), CoreError> {
        self.ask(|reply| Command::Remove(name, reply)).await
    }

    /// Forced kubeconfig setting; `None` restores the default behaviour.
    pub async fn set_kubeconfig(&self, path: Option<String>) -> Result<(), CoreError> {
        self.ask(|reply| Command::SetKubeconfig(path, reply)).await
    }

    pub async fn kubeconfig(&self) -> Result<Option<String>, CoreError> {
        self.ask(Command::Kubeconfig).await
    }

    /// Default Idle timeout, in minutes; `None`: never.
    pub async fn default_idle_timeout(&self) -> Result<Option<u32>, CoreError> {
        self.ask(Command::DefaultIdle).await
    }

    /// Changes the default Idle timeout; the affected Active Forwards are restarted.
    pub async fn set_default_idle_timeout(&self, minutes: Option<u32>) -> Result<(), CoreError> {
        self.ask(|reply| Command::SetDefaultIdle(minutes, reply))
            .await
    }
}

impl Core {
    async fn ask<T>(&self, make: impl FnOnce(Reply<T>) -> Command) -> Result<T, CoreError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(make(reply))
            .map_err(|_| CoreError::Stopped)?;
        response.await.map_err(|_| CoreError::Stopped)?
    }
}

fn config_error(e: ConfigError) -> CoreError {
    match e {
        ConfigError::DuplicateName(name) => CoreError::DuplicateName { name },
        ConfigError::MissingField(field) => CoreError::MissingField {
            field: match field {
                Field::Name => FormField::Name,
                Field::Context => FormField::Context,
                Field::Namespace => FormField::Namespace,
                Field::Target => FormField::Target,
            },
        },
        ConfigError::PortZero => CoreError::PortZero,
        other => CoreError::ConfigFile {
            detail: other.to_string(),
        },
    }
}

fn run(
    config: Config,
    path: PathBuf,
    mut commands: mpsc::UnboundedReceiver<Command>,
    listener: Box<dyn CoreListener>,
) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            return listener.notice(Notice::CoreStartFailed {
                detail: e.to_string(),
            });
        }
    };
    // Read here, not in the constructor: the login shell can take up to 5 s.
    let env = KubeEnv::from(login_shell_env());

    runtime.block_on(async move {
        let mut session = Session::new(config, path, env);
        session.restore().await;
        let mut statuses = session.statuses();
        let mut last = (
            views(&session, &statuses.borrow_and_update()),
            session.can_restore(),
        );
        listener.forwards_changed(last.0.clone(), last.1);

        loop {
            tokio::select! {
                command = commands.recv() => {
                    let Some(command) = command else { break };
                    for notice in handle(&mut session, command).await {
                        listener.notice(notice.into());
                    }
                }
                changed = statuses.changed() => {
                    if changed.is_err() {
                        break;
                    }
                }
                Some(notice) = session.next_expiry() => listener.notice(notice.into()),
            }
            // A command can change the config without changing the statuses (reload):
            // recompute after every event and send only if the view changed.
            let now = (
                views(&session, &statuses.borrow_and_update()),
                session.can_restore(),
            );
            if now != last {
                listener.forwards_changed(now.0.clone(), now.1);
                last = now;
            }
        }
    });
}

/// Runs one command on the Session; replies go through each command's `oneshot`.
async fn handle(session: &mut Session, command: Command) -> Vec<kubeport_core::Notice> {
    match command {
        Command::Activate(name) => return session.activate(&name).await,
        Command::Deactivate(name) => session.deactivate(&name),
        Command::SetAll(on) => return session.set_all(on).await,
        Command::ReconnectAll => session.reconnect_all().await,
        Command::Reload => return session.reload().await.into_iter().collect(),
        Command::Contexts(reply) => {
            let _ = reply.send(session.contexts().map_err(cluster_error));
        }
        Command::Namespaces(context, reply) => match session.client(&context).await {
            // The network call runs separately: the loop keeps serving the statuses.
            Ok(client) => {
                tokio::spawn(async move {
                    let _ = reply.send(catalog::namespaces(client).await.map_err(list_error));
                });
            }
            Err(e) => {
                let _ = reply.send(Err(cluster_error(e)));
            }
        },
        Command::Targets(context, namespace, reply) => match session.client(&context).await {
            Ok(client) => {
                tokio::spawn(async move {
                    let result = catalog::targets(client, &namespace).await;
                    let _ = reply.send(
                        result
                            .map(|t| t.into_iter().map(target_option).collect())
                            .map_err(list_error),
                    );
                });
            }
            Err(e) => {
                let _ = reply.send(Err(cluster_error(e)));
            }
        },
        Command::Save(previous, input, reply) => {
            return match session
                .save_forward(previous.as_deref(), forward(input))
                .await
            {
                Ok(notices) => {
                    let _ = reply.send(Ok(()));
                    notices
                }
                Err(e) => {
                    let _ = reply.send(Err(config_error(e)));
                    Vec::new()
                }
            };
        }
        Command::Remove(name, reply) => {
            let _ = reply.send(session.remove_forward(&name).await.map_err(config_error));
        }
        Command::SetKubeconfig(kubeconfig, reply) => {
            let path = kubeconfig.map(|p| PathBuf::from(p.trim()));
            let _ = reply.send(session.set_kubeconfig(path).await.map_err(config_error));
        }
        Command::Kubeconfig(reply) => {
            let current = session
                .config()
                .kubeconfig
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned());
            let _ = reply.send(Ok(current));
        }
        Command::DefaultIdle(reply) => {
            let _ = reply.send(Ok(session.config().idle_timeout_minutes));
        }
        Command::SetDefaultIdle(minutes, reply) => {
            let _ = reply.send(
                session
                    .set_default_idle_timeout(minutes)
                    .await
                    .map_err(config_error),
            );
        }
    }
    Vec::new()
}

/// A Forward entered in the form; surrounding spaces are not part of any field.
fn forward(input: ForwardInput) -> Forward {
    let target = input.target_name.trim().to_owned();
    Forward {
        name: input.name.trim().to_owned(),
        context: input.context.trim().to_owned(),
        namespace: input.namespace.trim().to_owned(),
        target: match input.target_kind {
            TargetKind::Service => Target::Service(target),
            TargetKind::Pod => Target::Pod(target),
        },
        remote_port: input.remote_port,
        local_port: input.local_port,
        idle_timeout: input.idle_timeout.into(),
    }
}

fn cluster_error(reason: Reason) -> CoreError {
    CoreError::Cluster {
        reason: reason.into(),
    }
}

fn list_error(detail: String) -> CoreError {
    CoreError::ClusterList { detail }
}

fn target_option(choice: TargetChoice) -> TargetOption {
    let (kind, name) = match choice.target {
        Target::Service(n) => (TargetKind::Service, n),
        Target::Pod(n) => (TargetKind::Pod, n),
    };
    TargetOption {
        kind,
        name,
        ports: choice
            .ports
            .into_iter()
            .map(|p| PortOption {
                port: p.port,
                name: p.name,
            })
            .collect(),
    }
}

fn views(session: &Session, statuses: &Statuses) -> Vec<ForwardView> {
    session
        .config()
        .forwards
        .iter()
        .map(|f| {
            let status = statuses.get(&f.name).cloned().unwrap_or_default();
            let (target_kind, target_name) = match &f.target {
                Target::Service(s) => (TargetKind::Service, s.clone()),
                Target::Pod(p) => (TargetKind::Pod, p.clone()),
            };
            ForwardView {
                name: f.name.clone(),
                context: f.context.clone(),
                namespace: f.namespace.clone(),
                target_kind,
                target_name,
                remote_port: f.remote_port,
                local_port: f.local_port,
                idle_timeout: f.idle_timeout.into(),
                idle_deadline_unix: status.idle_deadline.and_then(|t| {
                    t.duration_since(std::time::UNIX_EPOCH)
                        .ok()
                        .and_then(|d| i64::try_from(d.as_secs()).ok())
                }),
                active: status.active,
                state: status.real.map(|r| match r {
                    RealState::Connected { pod } => RealStateView::Connected { pod },
                    RealState::Reconnecting(reason) => RealStateView::Reconnecting {
                        reason: reason.into(),
                    },
                    RealState::Error(reason) => RealStateView::Error {
                        reason: reason.into(),
                    },
                }),
            }
        })
        .collect()
}
