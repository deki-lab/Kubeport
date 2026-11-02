//! Pilote les Forwards : État voulu, Forwards concurrents, un client par Contexte.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;

use kube::config::{KubeConfigOptions, Kubeconfig};
use kube::{Client, Config as KubeConfig};
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::config::Config;
use crate::forward;
use crate::state::{ForwardStatus, RealState};

pub type Statuses = BTreeMap<String, ForwardStatus>;

#[derive(Debug, thiserror::Error)]
pub enum ManagerError {
    #[error("aucun Forward nommé « {0} »")]
    UnknownForward(String),
}

pub struct Manager {
    config: Config,
    kubeconfig: Option<PathBuf>,
    clients: HashMap<String, Client>,
    running: HashMap<String, (CancellationToken, JoinHandle<()>)>,
    status: watch::Sender<Statuses>,
}

impl Manager {
    /// `kubeconfig` : chemin forcé ; `None` suit `KUBECONFIG` puis `~/.kube/config`.
    pub fn new(config: Config, kubeconfig: Option<PathBuf>) -> Self {
        let statuses = config
            .forwards
            .iter()
            .map(|f| (f.name.clone(), ForwardStatus::default()))
            .collect();
        Self {
            config,
            kubeconfig,
            clients: HashMap::new(),
            running: HashMap::new(),
            status: watch::channel(statuses).0,
        }
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn subscribe(&self) -> watch::Receiver<Statuses> {
        self.status.subscribe()
    }

    /// Passe un Forward à Actif. Renvoie les Forwards concurrents désactivés au passage.
    pub async fn activate(&mut self, name: &str) -> Result<Vec<String>, ManagerError> {
        let forward = self
            .config
            .get(name)
            .cloned()
            .ok_or_else(|| ManagerError::UnknownForward(name.to_owned()))?;
        if self.running.contains_key(name) {
            return Ok(Vec::new());
        }

        let rivals: Vec<String> = self
            .config
            .rivals(name)
            .into_iter()
            .map(|f| f.name.clone())
            .filter(|n| self.running.contains_key(n))
            .collect();
        for rival in &rivals {
            // Attendre la fin du concurrent : il doit libérer le port local avant qu'on l'ouvre.
            if let Some(task) = self.stop(rival) {
                let _ = task.await;
            }
        }

        let token = CancellationToken::new();
        let report = self.reporter(name, token.clone());
        report(RealState::Reconnecting("connexion au cluster".into()));

        let task = match self.client(&forward.context).await {
            Ok(client) => tokio::spawn(forward::run(forward, client, report, token.clone())),
            Err(reason) => {
                report(RealState::Error(reason));
                tokio::spawn(async {})
            }
        };
        self.running.insert(name.to_owned(), (token, task));
        Ok(rivals)
    }

    /// Passe un Forward à Inactif : ses connexions sont coupées.
    pub fn deactivate(&mut self, name: &str) {
        self.stop(name);
    }

    /// Annule un Forward, publie Inactif et rend sa tâche pour qui veut attendre sa fin.
    fn stop(&mut self, name: &str) -> Option<JoinHandle<()>> {
        let task = self.running.remove(name).map(|(token, task)| {
            // Annuler avant d'écrire : un rapport tardif verra le jeton annulé.
            token.cancel();
            task
        });
        self.status.send_if_modified(|statuses| {
            let Some(status) = statuses.get_mut(name) else {
                return false;
            };
            let changed = status.active || status.real.is_some();
            *status = ForwardStatus::default();
            changed
        });
        task
    }

    pub fn deactivate_all(&mut self) {
        let names: Vec<String> = self.running.keys().cloned().collect();
        for name in names {
            self.deactivate(&name);
        }
    }

    /// Publie l'État réel d'un Forward, sauf s'il a été désactivé entre-temps.
    fn reporter(&self, name: &str, token: CancellationToken) -> forward::Report {
        let status = self.status.clone();
        let name = name.to_owned();
        Arc::new(move |real: RealState| {
            status.send_if_modified(|statuses| {
                if token.is_cancelled() {
                    return false;
                }
                let entry = statuses.entry(name.clone()).or_default();
                let next = ForwardStatus {
                    active: true,
                    real: Some(real),
                };
                let changed = *entry != next;
                *entry = next;
                changed
            });
        })
    }

    async fn client(&mut self, context: &str) -> Result<Client, String> {
        if let Some(client) = self.clients.get(context) {
            return Ok(client.clone());
        }
        let kubeconfig = match &self.kubeconfig {
            Some(path) => Kubeconfig::read_from(path),
            None => Kubeconfig::read(),
        }
        .map_err(|e| format!("kubeconfig : {e}"))?;
        if !kubeconfig.contexts.iter().any(|c| c.name == context) {
            return Err(format!("Contexte {context} absent du kubeconfig"));
        }
        let options = KubeConfigOptions {
            context: Some(context.to_owned()),
            ..Default::default()
        };
        let config = KubeConfig::from_custom_kubeconfig(kubeconfig, &options)
            .await
            .map_err(|e| format!("Contexte {context} : {e}"))?;
        let client = Client::try_from(config).map_err(|e| format!("Contexte {context} : {e}"))?;
        self.clients.insert(context.to_owned(), client.clone());
        Ok(client)
    }
}

impl Drop for Manager {
    fn drop(&mut self) {
        for (token, _) in self.running.values() {
            token.cancel();
        }
    }
}
