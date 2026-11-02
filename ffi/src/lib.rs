//! Pont UniFFI entre `kubeport-core` et l'app SwiftUI.
//!
//! Swift ne voit que des Forwards, leur État voulu et leur État réel (ADR-0003).
//! Le cœur tourne sur son propre thread avec un runtime tokio `current_thread` ;
//! Swift lui envoie des commandes et reçoit les changements par `CoreListener`.
//! Les requêtes du formulaire (namespaces, Cibles, enregistrement) répondent par un
//! `oneshot`, attendu côté Swift comme une fonction `async`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use kubeport_core::catalog::{self, TargetChoice};
use kubeport_core::{
    Config, Forward, KubeEnv, Manager, RealState, Statuses, Target, login_shell_env, session,
};
use tokio::sync::{mpsc, oneshot};

uniffi::setup_scaffolding!();

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum TargetKind {
    Service,
    Pod,
}

/// État réel d'un Forward Actif.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum RealStateView {
    Connected { pod: String },
    Reconnecting { reason: String },
    Error { reason: String },
}

/// Un Forward tel que l'interface l'affiche.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ForwardView {
    pub name: String,
    pub context: String,
    pub namespace: String,
    pub target_kind: TargetKind,
    pub target_name: String,
    pub remote_port: u16,
    pub local_port: u16,
    /// État voulu.
    pub active: bool,
    /// État réel, présent seulement si le Forward est Actif.
    pub state: Option<RealStateView>,
}

/// Un Forward saisi dans le formulaire.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ForwardInput {
    pub name: String,
    pub context: String,
    pub namespace: String,
    pub target_kind: TargetKind,
    pub target_name: String,
    pub remote_port: u16,
    pub local_port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PortOption {
    pub port: u16,
    pub name: Option<String>,
}

/// Cible proposée par le cluster au formulaire.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TargetOption {
    pub kind: TargetKind,
    pub name: String,
    pub ports: Vec<PortOption>,
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum CoreError {
    #[error("{message}")]
    Config { message: String },
    #[error("{message}")]
    Cluster { message: String },
    #[error("cœur arrêté")]
    Stopped,
}

/// Implémenté côté Swift. Appelé depuis le thread du cœur, jamais le thread principal.
#[uniffi::export(callback_interface)]
pub trait CoreListener: Send + Sync {
    /// Liste complète des Forwards, dans l'ordre de la config, à chaque changement.
    fn forwards_changed(&self, forwards: Vec<ForwardView>);
    /// Message bref pour l'utilisateur (Forward concurrent désactivé, config invalide…).
    fn notice(&self, message: String);
}

type Reply<T> = oneshot::Sender<Result<T, CoreError>>;

enum Command {
    Activate(String),
    Deactivate(String),
    DeactivateAll,
    ReconnectAll,
    Reload,
    Contexts(Reply<Vec<String>>),
    Namespaces(String, Reply<Vec<String>>),
    Targets(String, String, Reply<Vec<TargetOption>>),
    Save(Option<String>, ForwardInput, Reply<()>),
    Remove(String, Reply<()>),
    SetKubeconfig(Option<String>, Reply<()>),
    Kubeconfig(Reply<Option<String>>),
}

#[derive(uniffi::Object)]
pub struct Core {
    commands: mpsc::UnboundedSender<Command>,
    config_path: String,
}

#[uniffi::export]
impl Core {
    /// Démarre le cœur. `config_path` absent : emplacement par défaut.
    /// Un fichier absent est créé avec un exemple commenté. Les Forwards Actifs à la
    /// dernière fermeture sont réactivés.
    #[uniffi::constructor]
    pub fn new(
        config_path: Option<String>,
        listener: Box<dyn CoreListener>,
    ) -> Result<Arc<Self>, CoreError> {
        let path = config_path
            .map(PathBuf::from)
            .unwrap_or_else(Config::default_path);
        ensure_config_file(&path)?;
        let config = Config::load(&path).map_err(config_error)?;

        let (commands, receiver) = mpsc::unbounded_channel();
        let thread_path = path.clone();
        std::thread::Builder::new()
            .name("kubeport-core".into())
            .spawn(move || run(config, thread_path, receiver, listener))
            .map_err(|e| CoreError::Config {
                message: format!("démarrage du cœur : {e}"),
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

    pub fn deactivate_all(&self) {
        let _ = self.commands.send(Command::DeactivateAll);
    }

    /// À appeler au réveil du Mac et à chaque changement de réseau.
    pub fn reconnect_all(&self) {
        let _ = self.commands.send(Command::ReconnectAll);
    }

    /// Relit le fichier de config après une édition à la main.
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

    /// Ajoute un Forward, ou remplace `previous_name`. Un Forward Actif modifié est relancé.
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

    /// Réglage du kubeconfig forcé ; `None` revient au comportement par défaut.
    pub async fn set_kubeconfig(&self, path: Option<String>) -> Result<(), CoreError> {
        self.ask(|reply| Command::SetKubeconfig(path, reply)).await
    }

    pub async fn kubeconfig(&self) -> Result<Option<String>, CoreError> {
        self.ask(Command::Kubeconfig).await
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

const TEMPLATE: &str = r#"# Forwards Kubeport. Un bloc [[forward]] par port-forward.
#
# [[forward]]
# name = "postgres-staging"   # nom unique
# context = "staging"          # contexte du kubeconfig
# namespace = "db"
# service = "postgres"         # ou : pod = "postgres-0"
# remote_port = 5432           # port du Service (ou du pod)
# local_port = 5432            # optionnel, vaut remote_port par défaut
#
# kubeconfig = "/chemin/vers/kubeconfig"   # optionnel, en tête de fichier
"#;

fn ensure_config_file(path: &Path) -> Result<(), CoreError> {
    if path.exists() {
        return Ok(());
    }
    let io = |e: std::io::Error| CoreError::Config {
        message: format!("{} : {e}", path.display()),
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(io)?;
    }
    std::fs::write(path, TEMPLATE).map_err(io)
}

fn config_error(e: impl std::fmt::Display) -> CoreError {
    CoreError::Config {
        message: e.to_string(),
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
        Err(e) => return listener.notice(format!("démarrage du cœur : {e}")),
    };
    // Lu ici et non dans le constructeur : le shell de login peut prendre jusqu'à 5 s.
    let env = KubeEnv::from(login_shell_env());
    let session_path = session::path_next_to(&path);

    runtime.block_on(async move {
        let mut manager = Manager::new(config, env);
        let mut statuses = manager.subscribe();
        for name in session::load_active(&session_path) {
            if manager.config().get(&name).is_some() {
                let _ = manager.activate(&name).await;
            }
        }
        let mut last = views(&manager, &statuses.borrow_and_update());
        let mut saved_active = active_names(&last);
        listener.forwards_changed(last.clone());

        loop {
            tokio::select! {
                command = commands.recv() => {
                    let Some(command) = command else { break };
                    handle(&mut manager, &path, command, listener.as_ref()).await;
                }
                changed = statuses.changed() => {
                    if changed.is_err() {
                        break;
                    }
                }
            }
            // Une commande peut changer la config sans changer les statuts (rechargement) :
            // on recalcule après chaque événement et on n'envoie que si la vue a changé.
            let now = views(&manager, &statuses.borrow_and_update());
            if now != last {
                let active = active_names(&now);
                if active != saved_active {
                    if let Err(e) = session::save_active(&session_path, &active) {
                        tracing::warn!("state.toml : {e}");
                    }
                    saved_active = active;
                }
                listener.forwards_changed(now.clone());
                last = now;
            }
        }
        // Fermeture de l'app : on garde state.toml tel quel pour la prochaine session.
        manager.deactivate_all();
    });
}

fn active_names(views: &[ForwardView]) -> Vec<String> {
    views
        .iter()
        .filter(|v| v.active)
        .map(|v| v.name.clone())
        .collect()
}

async fn handle(manager: &mut Manager, path: &Path, command: Command, listener: &dyn CoreListener) {
    match command {
        Command::Activate(name) => match manager.activate(&name).await {
            Ok(rivals) => {
                for rival in rivals {
                    listener.notice(format!(
                        "{rival} désactivé : port local partagé avec {name}"
                    ));
                }
            }
            Err(e) => listener.notice(e.to_string()),
        },
        Command::Deactivate(name) => manager.deactivate(&name),
        Command::DeactivateAll => manager.deactivate_all(),
        Command::ReconnectAll => manager.reconnect_all().await,
        Command::Reload => match Config::load(path) {
            Ok(config) => manager.replace_config(config).await,
            Err(e) => listener.notice(format!("config non rechargée : {e}")),
        },
        Command::Contexts(reply) => {
            let _ = reply.send(manager.contexts().map_err(cluster_error));
        }
        Command::Namespaces(context, reply) => match manager.client(&context).await {
            // L'appel réseau tourne à part : la boucle continue de servir les statuts.
            Ok(client) => {
                tokio::spawn(async move {
                    let _ = reply.send(catalog::namespaces(client).await.map_err(cluster_error));
                });
            }
            Err(e) => {
                let _ = reply.send(Err(cluster_error(e)));
            }
        },
        Command::Targets(context, namespace, reply) => match manager.client(&context).await {
            Ok(client) => {
                tokio::spawn(async move {
                    let result = catalog::targets(client, &namespace).await;
                    let _ = reply.send(
                        result
                            .map(|t| t.into_iter().map(target_option).collect())
                            .map_err(cluster_error),
                    );
                });
            }
            Err(e) => {
                let _ = reply.send(Err(cluster_error(e)));
            }
        },
        Command::Save(previous, input, reply) => {
            let _ = reply.send(save(manager, path, previous, input).await);
        }
        Command::Remove(name, reply) => {
            let mut config = manager.config().clone();
            config.remove(&name);
            let _ = reply.send(apply(manager, path, config).await);
        }
        Command::SetKubeconfig(kubeconfig, reply) => {
            let mut config = manager.config().clone();
            config.kubeconfig = kubeconfig
                .filter(|p| !p.trim().is_empty())
                .map(PathBuf::from);
            // replace_config vide le cache de clients : le nouveau kubeconfig est relu.
            let _ = reply.send(apply(manager, path, config).await);
        }
        Command::Kubeconfig(reply) => {
            let current = manager
                .config()
                .kubeconfig
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned());
            let _ = reply.send(Ok(current));
        }
    }
}

async fn save(
    manager: &mut Manager,
    path: &Path,
    previous: Option<String>,
    input: ForwardInput,
) -> Result<(), CoreError> {
    let forward = Forward {
        name: input.name.trim().to_owned(),
        context: input.context,
        namespace: input.namespace.trim().to_owned(),
        target: match input.target_kind {
            TargetKind::Service => Target::Service(input.target_name),
            TargetKind::Pod => Target::Pod(input.target_name),
        },
        remote_port: input.remote_port,
        local_port: input.local_port,
    };
    let was_active = previous.as_deref().is_some_and(|p| {
        manager
            .subscribe()
            .borrow()
            .get(p)
            .is_some_and(|s| s.active)
    });
    let name = forward.name.clone();
    let mut config = manager.config().clone();
    config
        .upsert(previous.as_deref(), forward)
        .map_err(config_error)?;
    apply(manager, path, config).await?;
    if was_active {
        let _ = manager.activate(&name).await;
    }
    Ok(())
}

/// Enregistre la config puis l'applique au Manager.
async fn apply(manager: &mut Manager, path: &Path, config: Config) -> Result<(), CoreError> {
    config.save(path).map_err(config_error)?;
    manager.replace_config(config).await;
    Ok(())
}

fn cluster_error(message: String) -> CoreError {
    CoreError::Cluster { message }
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

fn views(manager: &Manager, statuses: &Statuses) -> Vec<ForwardView> {
    manager
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
                active: status.active,
                state: status.real.map(|r| match r {
                    RealState::Connected { pod } => RealStateView::Connected { pod },
                    RealState::Reconnecting(reason) => RealStateView::Reconnecting { reason },
                    RealState::Error(reason) => RealStateView::Error { reason },
                }),
            }
        })
        .collect()
}
