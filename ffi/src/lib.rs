//! Pont UniFFI entre `kubeport-core` et l'app SwiftUI.
//!
//! Swift ne voit que des Forwards, leur État voulu et leur État réel (ADR-0003).
//! Le cœur tourne sur son propre thread avec un runtime tokio `current_thread` ;
//! Swift lui envoie des commandes et reçoit les changements par `CoreListener`.

use std::path::PathBuf;
use std::sync::Arc;

use kubeport_core::{Config, KubeEnv, Manager, RealState, Statuses, Target, login_shell_env};
use tokio::sync::mpsc;

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

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum CoreError {
    #[error("{message}")]
    Config { message: String },
}

/// Implémenté côté Swift. Appelé depuis le thread du cœur, jamais le thread principal.
#[uniffi::export(callback_interface)]
pub trait CoreListener: Send + Sync {
    /// Liste complète des Forwards, dans l'ordre de la config, à chaque changement.
    fn forwards_changed(&self, forwards: Vec<ForwardView>);
    /// Message bref pour l'utilisateur (Forward concurrent désactivé, config invalide…).
    fn notice(&self, message: String);
}

enum Command {
    Activate(String),
    Deactivate(String),
    DeactivateAll,
    ReconnectAll,
    Reload,
}

#[derive(uniffi::Object)]
pub struct Core {
    commands: mpsc::UnboundedSender<Command>,
    config_path: String,
}

#[uniffi::export]
impl Core {
    /// Démarre le cœur. `config_path` absent : emplacement par défaut.
    /// Un fichier absent est créé avec un exemple commenté.
    #[uniffi::constructor]
    pub fn new(
        config_path: Option<String>,
        listener: Box<dyn CoreListener>,
    ) -> Result<Arc<Self>, CoreError> {
        let path = config_path
            .map(PathBuf::from)
            .unwrap_or_else(Config::default_path);
        ensure_config_file(&path)?;
        let config = Config::load(&path).map_err(|e| CoreError::Config {
            message: e.to_string(),
        })?;

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

fn ensure_config_file(path: &std::path::Path) -> Result<(), CoreError> {
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

    runtime.block_on(async move {
        let mut manager = Manager::new(config, env);
        let mut statuses = manager.subscribe();
        let mut last = views(&manager, &statuses.borrow_and_update());
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
                listener.forwards_changed(now.clone());
                last = now;
            }
        }
        manager.deactivate_all();
    });
}

async fn handle(
    manager: &mut Manager,
    path: &std::path::Path,
    command: Command,
    listener: &dyn CoreListener,
) {
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
