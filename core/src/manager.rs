//! Pilote les Forwards : État voulu, Forwards concurrents, un client par Contexte.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;

use kube::config::{KubeConfigOptions, Kubeconfig};
use kube::{Client, Config as KubeConfig};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::config::Config;
use crate::forward;
use crate::shell_env::ShellEnv;
use crate::state::{ForwardStatus, RealState, Reason};

pub type Statuses = BTreeMap<String, ForwardStatus>;

#[derive(Debug, thiserror::Error)]
pub enum ManagerError {
    #[error("no Forward named {0:?}")]
    UnknownForward(String),
}

/// D'où lire le kubeconfig et avec quel `PATH` lancer les plugins d'auth `exec`.
#[derive(Debug, Default, Clone)]
pub struct KubeEnv {
    /// Fichiers kubeconfig à fusionner ; vide : `KUBECONFIG` du process, puis `~/.kube/config`.
    pub kubeconfig: Vec<PathBuf>,
    /// `PATH` injecté dans les plugins `exec` qui n'en déclarent pas.
    pub exec_path: Option<String>,
}

impl From<ShellEnv> for KubeEnv {
    fn from(env: ShellEnv) -> Self {
        Self {
            kubeconfig: env.kubeconfig,
            exec_path: env.path,
        }
    }
}

pub struct Manager {
    config: Config,
    env: KubeEnv,
    clients: HashMap<String, Client>,
    running: HashMap<String, (CancellationToken, JoinHandle<()>)>,
    status: watch::Sender<Statuses>,
    expired_tx: mpsc::UnboundedSender<String>,
    expired_rx: Option<mpsc::UnboundedReceiver<String>>,
}

impl Manager {
    pub fn new(config: Config, env: KubeEnv) -> Self {
        let statuses = initial_statuses(&config);
        let (expired_tx, expired_rx) = mpsc::unbounded_channel();
        Self {
            config,
            env,
            clients: HashMap::new(),
            running: HashMap::new(),
            status: watch::channel(statuses).0,
            expired_tx,
            expired_rx: Some(expired_rx),
        }
    }

    /// Noms des Forwards dont le Délai d'inactivité vient d'expirer. Celui qui pilote le
    /// Manager les reçoit et appelle `deactivate` : le Manager ne se désactive pas seul,
    /// pour que l'interface puisse prévenir l'utilisateur. Disponible une seule fois.
    pub fn take_expired(&mut self) -> Option<mpsc::UnboundedReceiver<String>> {
        self.expired_rx.take()
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
        report(RealState::Reconnecting(Reason::ConnectingToCluster));
        let idle = self.idle(name, &forward, token.clone());

        let task = match self.client(&forward.context).await {
            Ok(client) => tokio::spawn(forward::run(forward, client, report, idle, token.clone())),
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

    pub fn deactivate_all(&mut self) {
        let names: Vec<String> = self.running.keys().cloned().collect();
        for name in names {
            self.deactivate(&name);
        }
    }

    /// Relance tous les Forwards Actifs avec des clients neufs (réveil, changement de réseau).
    pub async fn reconnect_all(&mut self) {
        let names = self.stop_all().await;
        self.clients.clear();
        for name in names {
            let _ = self.activate(&name).await;
        }
    }

    /// Remplace la config (fichier édité à la main). Les Forwards Actifs restés identiques
    /// sont relancés ; ceux supprimés ou modifiés deviennent Inactifs.
    pub async fn replace_config(&mut self, config: Config) {
        let keep: Vec<String> = self
            .running
            .keys()
            .filter(|name| config.get(name) == self.config.get(name))
            .cloned()
            .collect();
        self.stop_all().await;
        self.clients.clear();
        self.status.send_replace(initial_statuses(&config));
        self.config = config;
        for name in keep {
            let _ = self.activate(&name).await;
        }
    }

    /// Arrête tous les Forwards Actifs, attend leur fin, renvoie leurs noms.
    async fn stop_all(&mut self) -> Vec<String> {
        let mut names: Vec<String> = self.running.keys().cloned().collect();
        names.sort();
        for name in &names {
            if let Some(task) = self.stop(name) {
                let _ = task.await;
            }
        }
        names
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
            let changed = *status != ForwardStatus::default();
            *status = ForwardStatus::default();
            changed
        });
        task
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
                let changed = !entry.active || entry.real.as_ref() != Some(&real);
                entry.active = true;
                entry.real = Some(real);
                changed
            });
        })
    }

    /// Branche le Délai d'inactivité d'un Forward sur les statuts et sur `take_expired`.
    fn idle(
        &self,
        name: &str,
        forward: &crate::config::Forward,
        token: CancellationToken,
    ) -> forward::Idle {
        let status = self.status.clone();
        let deadline_name = name.to_owned();
        let deadline_token = token.clone();
        let expired_tx = self.expired_tx.clone();
        let expired_name = name.to_owned();
        forward::Idle {
            timeout: self.config.idle_timeout(forward),
            deadline: Arc::new(move |deadline| {
                status.send_if_modified(|statuses| {
                    if deadline_token.is_cancelled() {
                        return false;
                    }
                    let entry = statuses.entry(deadline_name.clone()).or_default();
                    let changed = entry.idle_deadline != deadline;
                    entry.idle_deadline = deadline;
                    changed
                });
            }),
            expired: Arc::new(move || {
                if !token.is_cancelled() {
                    let _ = expired_tx.send(expired_name.clone());
                }
            }),
        }
    }

    /// Noms des Contextes du kubeconfig, triés.
    pub fn contexts(&self) -> Result<Vec<String>, Reason> {
        let mut names: Vec<String> = self
            .read_kubeconfig()?
            .contexts
            .into_iter()
            .map(|c| c.name)
            .collect();
        names.sort();
        Ok(names)
    }

    /// Client du Contexte, mis en cache ; pour les appels ponctuels du formulaire.
    pub async fn client(&mut self, context: &str) -> Result<Client, Reason> {
        if let Some(client) = self.clients.get(context) {
            return Ok(client.clone());
        }
        let mut kubeconfig = self.read_kubeconfig()?;
        if !kubeconfig.contexts.iter().any(|c| c.name == context) {
            return Err(Reason::ContextMissing {
                context: context.to_owned(),
            });
        }
        if let Some(path) = &self.env.exec_path {
            inject_exec_path(&mut kubeconfig, path);
        }
        let options = KubeConfigOptions {
            context: Some(context.to_owned()),
            ..Default::default()
        };
        let config = KubeConfig::from_custom_kubeconfig(kubeconfig, &options)
            .await
            .map_err(|e| Reason::Kubeconfig {
                detail: format!("{context}: {e}"),
            })?;
        let client = Client::try_from(config).map_err(|e| Reason::Kubeconfig {
            detail: format!("{context}: {e}"),
        })?;
        self.clients.insert(context.to_owned(), client.clone());
        Ok(client)
    }

    /// Ordre de priorité : réglage `kubeconfig` de la config, `KUBECONFIG` du shell de
    /// login, puis comportement standard (`KUBECONFIG` du process, `~/.kube/config`).
    fn read_kubeconfig(&self) -> Result<Kubeconfig, Reason> {
        let paths: Vec<PathBuf> = match &self.config.kubeconfig {
            Some(path) => vec![path.clone()],
            None => self.env.kubeconfig.clone(),
        };
        let result = if paths.is_empty() {
            Kubeconfig::read()
        } else {
            paths
                .iter()
                .map(Kubeconfig::read_from)
                .try_fold(Kubeconfig::default(), |acc, next| acc.merge(next?))
        };
        result.map_err(|e| Reason::Kubeconfig {
            detail: e.to_string(),
        })
    }
}

fn initial_statuses(config: &Config) -> Statuses {
    config
        .forwards
        .iter()
        .map(|f| (f.name.clone(), ForwardStatus::default()))
        .collect()
}

/// Donne aux plugins `exec` (aws, gke-gcloud-auth-plugin…) le `PATH` du shell de login,
/// sauf s'ils en déclarent déjà un. `Command` cherche alors le binaire dans ce `PATH`.
fn inject_exec_path(kubeconfig: &mut Kubeconfig, path: &str) {
    for exec in kubeconfig
        .auth_infos
        .iter_mut()
        .filter_map(|a| a.auth_info.as_mut()?.exec.as_mut())
    {
        let env = exec.env.get_or_insert_default();
        if !env
            .iter()
            .any(|e| e.get("name").map(String::as_str) == Some("PATH"))
        {
            env.push(HashMap::from([
                ("name".to_owned(), "PATH".to_owned()),
                ("value".to_owned(), path.to_owned()),
            ]));
        }
    }
}

impl Drop for Manager {
    fn drop(&mut self) {
        for (token, _) in self.running.values() {
            token.cancel();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn injecte_le_path_seulement_aux_plugins_exec_sans_path() {
        let mut kc = Kubeconfig::from_yaml(
            r#"
apiVersion: v1
kind: Config
users:
  - name: eks
    user:
      exec:
        apiVersion: client.authentication.k8s.io/v1beta1
        command: aws
  - name: deja
    user:
      exec:
        apiVersion: client.authentication.k8s.io/v1beta1
        command: kubelogin
        env:
          - name: PATH
            value: /custom
  - name: token
    user:
      token: abc
"#,
        )
        .unwrap();
        inject_exec_path(&mut kc, "/opt/homebrew/bin:/usr/bin");
        let path_of = |i: usize| {
            kc.auth_infos[i]
                .auth_info
                .as_ref()
                .unwrap()
                .exec
                .as_ref()
                .unwrap()
                .env
                .as_ref()
                .unwrap()
                .iter()
                .find(|e| e["name"] == "PATH")
                .map(|e| e["value"].clone())
        };
        assert_eq!(path_of(0).as_deref(), Some("/opt/homebrew/bin:/usr/bin"));
        assert_eq!(path_of(1).as_deref(), Some("/custom"));
        assert!(kc.auth_infos[2].auth_info.as_ref().unwrap().exec.is_none());
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;

    fn config(forwards: &[(&str, &str, u16)]) -> Config {
        let text: String = forwards
            .iter()
            .map(|(name, svc, port)| {
                format!(
                    "[[forward]]\nname = \"{name}\"\ncontext = \"c\"\nnamespace = \"n\"\nservice = \"{svc}\"\nremote_port = 80\nlocal_port = {port}\n\n"
                )
            })
            .collect();
        Config::parse(&text).unwrap()
    }

    /// Kubeconfig introuvable : chaque activation finit en Erreur, sans cluster ni réseau.
    fn manager(config: Config) -> Manager {
        let env = KubeEnv {
            kubeconfig: vec!["/nonexistent/kubeconfig".into()],
            exec_path: None,
        };
        Manager::new(config, env)
    }

    fn active(m: &Manager) -> Vec<String> {
        let statuses = m.subscribe().borrow().clone();
        statuses
            .into_iter()
            .filter(|(_, s)| s.active)
            .map(|(n, _)| n)
            .collect()
    }

    #[tokio::test(flavor = "current_thread")]
    async fn activation_sans_kubeconfig_donne_une_erreur() {
        let mut m = manager(config(&[("a", "s", 1)]));
        m.activate("a").await.unwrap();
        let status = m.subscribe().borrow()["a"].clone();
        assert!(status.active);
        assert!(matches!(
            status.real,
            Some(RealState::Error(Reason::Kubeconfig { .. }))
        ));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn rechargement_garde_seulement_les_forwards_inchanges() {
        let mut m = manager(config(&[
            ("garde", "s", 1),
            ("modifie", "s", 2),
            ("supprime", "s", 3),
        ]));
        for name in ["garde", "modifie", "supprime"] {
            m.activate(name).await.unwrap();
        }
        m.replace_config(config(&[
            ("garde", "s", 1),
            ("modifie", "autre", 2),
            ("nouveau", "s", 4),
        ]))
        .await;
        assert_eq!(active(&m), ["garde"]);
        let statuses = m.subscribe().borrow().clone();
        assert_eq!(
            statuses.keys().collect::<Vec<_>>(),
            ["garde", "modifie", "nouveau"]
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn reconnexion_garde_les_memes_forwards_actifs() {
        let mut m = manager(config(&[("a", "s", 1), ("b", "s", 2), ("c", "s", 3)]));
        m.activate("a").await.unwrap();
        m.activate("c").await.unwrap();
        m.reconnect_all().await;
        assert_eq!(active(&m), ["a", "c"]);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn activer_un_concurrent_desactive_l_autre() {
        let mut m = manager(config(&[("staging", "s", 5432), ("prod", "s", 5432)]));
        m.activate("staging").await.unwrap();
        let rivals = m.activate("prod").await.unwrap();
        assert_eq!(rivals, ["staging"]);
        assert_eq!(active(&m), ["prod"]);
    }
}

#[cfg(test)]
mod idle_tests {
    use std::time::Duration;

    use super::*;
    use crate::config::{Forward, IdleTimeout, Target};

    /// Avec docker-desktop et tests/fixtures/whoami.yaml appliqué :
    /// `cargo test -p kubeport-core -- --ignored`
    #[tokio::test(flavor = "current_thread")]
    #[ignore]
    async fn delai_d_inactivite_sur_docker_desktop() {
        let forward = Forward {
            name: "idle".into(),
            context: "docker-desktop".into(),
            namespace: "kubeport-it".into(),
            target: Target::Service("whoami".into()),
            remote_port: 8080,
            local_port: 28090,
            idle_timeout: IdleTimeout::After(Duration::from_secs(2)),
        };
        let config = Config {
            forwards: vec![forward],
            ..Default::default()
        };
        let mut m = Manager::new(config, KubeEnv::default());
        let mut expired = m.take_expired().unwrap();
        let mut statuses = m.subscribe();
        m.activate("idle").await.unwrap();

        let connected = |s: &Statuses| matches!(s["idle"].real, Some(RealState::Connected { .. }));
        tokio::time::timeout(Duration::from_secs(20), statuses.wait_for(connected))
            .await
            .expect("Connecté")
            .unwrap();
        assert!(
            m.subscribe().borrow()["idle"].idle_deadline.is_some(),
            "échéance armée"
        );

        // Une connexion ouverte suspend le délai, même bien au-delà de 2 s.
        let socket = tokio::net::TcpStream::connect(("127.0.0.1", 28090))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(
            m.subscribe().borrow()["idle"].idle_deadline.is_none(),
            "échéance suspendue"
        );
        assert!(
            tokio::time::timeout(Duration::from_secs(3), expired.recv())
                .await
                .is_err(),
            "pas d'expiration pendant une connexion"
        );

        drop(socket);
        let name = tokio::time::timeout(Duration::from_secs(10), expired.recv())
            .await
            .expect("expiration après fermeture")
            .unwrap();
        assert_eq!(name, "idle");
        m.deactivate(&name);
        assert!(!m.subscribe().borrow()["idle"].active);
    }
}
