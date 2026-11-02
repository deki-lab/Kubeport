//! Drives the Forwards: Desired state, Competing Forwards, one client per Context.

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

/// Where to read the kubeconfig from and which `PATH` to run `exec` auth plugins with.
#[derive(Debug, Default, Clone)]
pub struct KubeEnv {
    /// Kubeconfig files to merge; empty: the process `KUBECONFIG`, then `~/.kube/config`.
    pub kubeconfig: Vec<PathBuf>,
    /// `PATH` injected into `exec` plugins that do not declare one.
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

    /// Names of the Forwards whose Idle timeout just expired. Whoever drives the
    /// Manager receives them and calls `deactivate`: the Manager does not deactivate on its
    /// own, so the UI can notify the user. Available only once.
    pub fn take_expired(&mut self) -> Option<mpsc::UnboundedReceiver<String>> {
        self.expired_rx.take()
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn subscribe(&self) -> watch::Receiver<Statuses> {
        self.status.subscribe()
    }

    /// Makes a Forward Active. Returns the Competing Forwards deactivated along the way.
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
            // Wait for the competitor to finish: it must release the local port before we open it.
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

    /// Makes a Forward Inactive: its connections are cut.
    pub fn deactivate(&mut self, name: &str) {
        self.stop(name);
    }

    pub fn deactivate_all(&mut self) {
        let names: Vec<String> = self.running.keys().cloned().collect();
        for name in names {
            self.deactivate(&name);
        }
    }

    /// Restarts every Active Forward with fresh clients (wake-up, network change).
    pub async fn reconnect_all(&mut self) {
        let names = self.stop_all().await;
        self.clients.clear();
        for name in names {
            let _ = self.activate(&name).await;
        }
    }

    /// Replaces the config (hand-edited file). Active Forwards left unchanged are
    /// restarted; deleted or modified ones become Inactive.
    pub async fn replace_config(&mut self, config: Config) {
        // Only what changed is touched: other Active Forwards keep their connections.
        // A new kubeconfig invalidates every client, so everything restarts then.
        let new_kubeconfig = config.kubeconfig != self.config.kubeconfig;
        let mut running: Vec<String> = self.running.keys().cloned().collect();
        running.sort();
        let mut restart = Vec::new();
        for name in running {
            let old = self.config.get(&name);
            let new = config.get(&name);
            let unchanged = new.is_some() && old == new;
            let idle_changed = unchanged
                && self.config.idle_timeout(old.unwrap()) != config.idle_timeout(new.unwrap());
            if unchanged && !new_kubeconfig && !idle_changed {
                continue;
            }
            if let Some(task) = self.stop(&name) {
                let _ = task.await;
            }
            if unchanged {
                restart.push(name);
            }
        }
        if new_kubeconfig {
            self.clients.clear();
        }
        self.status.send_modify(|statuses| {
            statuses.retain(|name, _| config.get(name).is_some());
            for forward in &config.forwards {
                statuses.entry(forward.name.clone()).or_default();
            }
        });
        self.config = config;
        for name in restart {
            let _ = self.activate(&name).await;
        }
    }

    /// Names of the Active Forwards, sorted.
    pub fn active(&self) -> Vec<String> {
        let mut names: Vec<String> = self.running.keys().cloned().collect();
        names.sort();
        names
    }

    /// Stops every Active Forward, waits for them to finish, returns their names.
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

    /// Cancels a Forward, publishes Inactive and returns its task for whoever wants to await it.
    fn stop(&mut self, name: &str) -> Option<JoinHandle<()>> {
        let task = self.running.remove(name).map(|(token, task)| {
            // Cancel before writing: a late report will see the cancelled token.
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

    /// Publishes a Forward's Actual state, unless it was deactivated in the meantime.
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

    /// Wires a Forward's Idle timeout to the statuses and to `take_expired`.
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

    /// Names of the kubeconfig Contexts, sorted.
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

    /// Cached client for the Context; for the form's one-off calls.
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

    /// Priority order: the config's `kubeconfig` setting, the login shell's `KUBECONFIG`,
    /// then standard behaviour (the process `KUBECONFIG`, `~/.kube/config`).
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

/// Gives `exec` plugins (aws, gke-gcloud-auth-plugin…) the login shell's `PATH`,
/// unless they already declare one. `Command` then looks up the binary in that `PATH`.
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
    fn injects_path_only_into_exec_plugins_without_one() {
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
  - name: already
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

    /// Kubeconfig not found: every activation ends in Error, with no cluster or network.
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
    async fn activation_without_kubeconfig_gives_an_error() {
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
    async fn reload_keeps_only_unchanged_forwards() {
        let mut m = manager(config(&[
            ("kept", "s", 1),
            ("modified", "s", 2),
            ("deleted", "s", 3),
        ]));
        for name in ["kept", "modified", "deleted"] {
            m.activate(name).await.unwrap();
        }
        m.replace_config(config(&[
            ("kept", "s", 1),
            ("modified", "other", 2),
            ("added", "s", 4),
        ]))
        .await;
        assert_eq!(active(&m), ["kept"]);
        let statuses = m.subscribe().borrow().clone();
        assert_eq!(
            statuses.keys().collect::<Vec<_>>(),
            ["added", "kept", "modified"]
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn reconnect_keeps_the_same_active_forwards() {
        let mut m = manager(config(&[("a", "s", 1), ("b", "s", 2), ("c", "s", 3)]));
        m.activate("a").await.unwrap();
        m.activate("c").await.unwrap();
        m.reconnect_all().await;
        assert_eq!(active(&m), ["a", "c"]);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn activating_a_competing_forward_deactivates_the_other() {
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

    /// With docker-desktop and tests/fixtures/whoami.yaml applied:
    /// `cargo test -p kubeport-core -- --ignored`
    #[tokio::test(flavor = "current_thread")]
    #[ignore]
    async fn idle_timeout_on_docker_desktop() {
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
            .expect("Connected")
            .unwrap();
        assert!(
            m.subscribe().borrow()["idle"].idle_deadline.is_some(),
            "deadline armed"
        );

        // An open connection suspends the timeout, even well past 2 s.
        let socket = tokio::net::TcpStream::connect(("127.0.0.1", 28090))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(
            m.subscribe().borrow()["idle"].idle_deadline.is_none(),
            "deadline suspended"
        );
        assert!(
            tokio::time::timeout(Duration::from_secs(3), expired.recv())
                .await
                .is_err(),
            "no expiry while a connection is open"
        );

        drop(socket);
        let name = tokio::time::timeout(Duration::from_secs(10), expired.recv())
            .await
            .expect("expiry after close")
            .unwrap();
        assert_eq!(name, "idle");
        m.deactivate(&name);
        assert!(!m.subscribe().borrow()["idle"].active);
    }
}
