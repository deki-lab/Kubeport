//! Drives the Forwards: Desired state, Competing Forwards, one `Cluster` per Context.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::cluster::Connector;
use crate::cluster::kube::{KubeConnector, KubeEnv};
use crate::config::Config;
use crate::forward;
use crate::state::{ForwardStatus, RealState, Reason};

pub type Statuses = BTreeMap<String, ForwardStatus>;

#[derive(Debug, thiserror::Error)]
pub enum ManagerError {
    #[error("no Forward named {0:?}")]
    UnknownForward(String),
}

pub struct Manager<C: Connector = KubeConnector> {
    config: Config,
    connector: C,
    running: HashMap<String, (CancellationToken, JoinHandle<()>)>,
    status: watch::Sender<Statuses>,
    expired_tx: mpsc::UnboundedSender<String>,
    expired_rx: Option<mpsc::UnboundedReceiver<String>>,
}

impl Manager {
    pub fn new(config: Config, env: KubeEnv) -> Self {
        Self::with_connector(config, KubeConnector::new(env))
    }

    /// Cached kube-rs client for the Context; for the form's one-off listings.
    pub async fn client(&mut self, context: &str) -> Result<kube::Client, Reason> {
        let kubeconfig = self.config.kubeconfig.clone();
        self.connector.client(kubeconfig.as_deref(), context).await
    }
}

impl<C: Connector> Manager<C> {
    pub fn with_connector(config: Config, connector: C) -> Self {
        let statuses = initial_statuses(&config);
        let (expired_tx, expired_rx) = mpsc::unbounded_channel();
        Self {
            config,
            connector,
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

        let kubeconfig = self.config.kubeconfig.clone();
        let task = match self
            .connector
            .cluster(kubeconfig.as_deref(), &forward.context)
            .await
        {
            Ok(cluster) => {
                tokio::spawn(forward::run(forward, cluster, report, idle, token.clone()))
            }
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
        self.connector.forget();
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
            self.connector.forget();
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
        self.connector.contexts(self.config.kubeconfig.as_deref())
    }
}

fn initial_statuses(config: &Config) -> Statuses {
    config
        .forwards
        .iter()
        .map(|f| (f.name.clone(), ForwardStatus::default()))
        .collect()
}

impl<C: Connector> Drop for Manager<C> {
    fn drop(&mut self) {
        for (token, _) in self.running.values() {
            token.cancel();
        }
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

/// The Forward runtime through the Manager's interface, against the in-memory cluster:
/// no cluster, no network beyond 127.0.0.1.
#[cfg(test)]
mod runtime_tests {
    use std::time::Duration;

    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpStream;

    use super::*;
    use crate::cluster::Fault;
    use crate::cluster::memory::{MemoryCluster, MemoryConnector, pod, service};
    use crate::config::{Forward, IdleTimeout, Target};

    fn free_port() -> u16 {
        std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    fn forward(target: Target, idle: IdleTimeout) -> Forward {
        Forward {
            name: "f".into(),
            context: "c".into(),
            namespace: "n".into(),
            target,
            remote_port: 80,
            local_port: free_port(),
            idle_timeout: idle,
        }
    }

    fn manager(forward: Forward) -> (Manager<MemoryConnector>, MemoryCluster, u16) {
        let cluster = MemoryCluster::new();
        let mut connector = MemoryConnector::default();
        connector.clusters.insert("c".into(), cluster.clone());
        let port = forward.local_port;
        let config = Config {
            forwards: vec![forward],
            ..Default::default()
        };
        (Manager::with_connector(config, connector), cluster, port)
    }

    async fn wait(
        m: &Manager<MemoryConnector>,
        what: &str,
        ok: impl Fn(&Option<RealState>) -> bool,
    ) {
        let mut statuses = m.subscribe();
        let found = tokio::time::timeout(
            Duration::from_secs(5),
            statuses.wait_for(|s| ok(&s.get("f").and_then(|f| f.real.clone()))),
        )
        .await;
        assert!(
            found.is_ok(),
            "{what}: still {:?}",
            m.subscribe().borrow().get("f")
        );
    }

    fn connected_to(pod: &'static str) -> impl Fn(&Option<RealState>) -> bool {
        move |s| matches!(s, Some(RealState::Connected { pod: p }) if p == pod)
    }

    /// Opens a local connection and reads which pod served it.
    async fn open(port: u16) -> (String, BufReader<TcpStream>) {
        let mut stream = BufReader::new(TcpStream::connect(("127.0.0.1", port)).await.unwrap());
        let mut line = String::new();
        tokio::time::timeout(Duration::from_secs(5), stream.read_line(&mut line))
            .await
            .expect("pod answers")
            .unwrap();
        (line.trim().to_owned(), stream)
    }

    fn terminating(mut p: k8s_openapi::api::core::v1::Pod) -> k8s_openapi::api::core::v1::Pod {
        p.metadata.deletion_timestamp = p.metadata.creation_timestamp.clone();
        p
    }

    #[tokio::test(flavor = "current_thread")]
    async fn rollout_moves_new_connections_and_keeps_open_ones() {
        let (mut m, cluster, port) =
            manager(forward(Target::Service("web".into()), IdleTimeout::Never));
        cluster.set_service("n", service("web", "web"));
        cluster.apply_pod("n", pod("old", "web", 100, true));
        cluster.apply_pod("n", pod("young", "web", 200, true));
        m.activate("f").await.unwrap();
        wait(&m, "oldest Ready pod", connected_to("old")).await;

        let (served, mut kept) = open(port).await;
        assert_eq!(served, "old");

        // Rollout: the Resolved pod starts terminating.
        cluster.apply_pod("n", terminating(pod("old", "web", 100, true)));
        wait(&m, "switch to the other pod", connected_to("young")).await;
        assert_eq!(open(port).await.0, "young");

        // The connection opened before the switch still talks to its pod.
        kept.get_mut().write_all(b"ping").await.unwrap();
        let mut echo = [0u8; 4];
        kept.read_exact(&mut echo).await.unwrap();
        assert_eq!(&echo, b"ping");
        assert_eq!(cluster.connections(), ["old", "young"]);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn no_ready_pod_reconnects_until_one_shows_up() {
        let (mut m, cluster, _) =
            manager(forward(Target::Service("web".into()), IdleTimeout::Never));
        cluster.set_service("n", service("web", "web"));
        cluster.apply_pod("n", pod("starting", "web", 100, false));
        m.activate("f").await.unwrap();
        wait(&m, "no Ready pod", |s| {
            matches!(s, Some(RealState::Reconnecting(Reason::NoReadyPod)))
        })
        .await;
        cluster.apply_pod("n", pod("starting", "web", 100, true));
        wait(&m, "pod became Ready", connected_to("starting")).await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn missing_or_podless_services_are_errors() {
        let (mut m, _, _) = manager(forward(Target::Service("ghost".into()), IdleTimeout::Never));
        m.activate("f").await.unwrap();
        wait(&m, "Service not found", |s| {
            matches!(s, Some(RealState::Error(Reason::ServiceNotFound { service, .. })) if service == "ghost")
        })
        .await;

        let (mut m, cluster, _) =
            manager(forward(Target::Service("ext".into()), IdleTimeout::Never));
        let mut ext = service("ext", "x");
        ext.spec.as_mut().unwrap().type_ = Some("ExternalName".into());
        cluster.set_service("n", ext);
        m.activate("f").await.unwrap();
        wait(&m, "ExternalName", |s| {
            matches!(s, Some(RealState::Error(Reason::ExternalNameService)))
        })
        .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn a_dropped_watch_reconnects_then_recovers() {
        let (mut m, cluster, _) =
            manager(forward(Target::Service("web".into()), IdleTimeout::Never));
        cluster.set_service("n", service("web", "web"));
        cluster.apply_pod("n", pod("a", "web", 100, true));
        m.activate("f").await.unwrap();
        wait(&m, "connected", connected_to("a")).await;
        cluster.fail_watch(
            "n",
            Fault::Transient(Reason::ApiUnreachable {
                detail: "eof".into(),
            }),
        );
        wait(&m, "watch dropped", |s| {
            matches!(
                s,
                Some(RealState::Reconnecting(Reason::ApiUnreachable { .. }))
            )
        })
        .await;
        cluster.apply_pod("n", pod("a", "web", 100, true));
        wait(&m, "back", connected_to("a")).await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn a_refused_port_forward_is_an_error() {
        let (mut m, cluster, port) =
            manager(forward(Target::Service("web".into()), IdleTimeout::Never));
        cluster.set_service("n", service("web", "web"));
        cluster.apply_pod("n", pod("a", "web", 100, true));
        m.activate("f").await.unwrap();
        wait(&m, "connected", connected_to("a")).await;
        cluster.fail_next_connect(Fault::Fatal(Reason::AccessDenied {
            detail: "pods/portforward".into(),
        }));
        let _socket = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        wait(&m, "access denied", |s| {
            matches!(s, Some(RealState::Error(Reason::AccessDenied { .. })))
        })
        .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn idle_timeout_waits_for_the_last_connection_to_close() {
        let idle = IdleTimeout::After(Duration::from_millis(300));
        let (mut m, cluster, port) = manager(forward(Target::Service("web".into()), idle));
        cluster.set_service("n", service("web", "web"));
        cluster.apply_pod("n", pod("a", "web", 100, true));
        let mut expired = m.take_expired().unwrap();
        m.activate("f").await.unwrap();
        wait(&m, "connected", connected_to("a")).await;

        let (_, socket) = open(port).await;
        assert!(
            tokio::time::timeout(Duration::from_millis(800), expired.recv())
                .await
                .is_err(),
            "no expiry while a connection is open"
        );
        drop(socket);
        let name = tokio::time::timeout(Duration::from_secs(3), expired.recv())
            .await
            .expect("expiry after the connection closed")
            .unwrap();
        assert_eq!(name, "f");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn a_named_pod_comes_back_under_the_same_name() {
        let (mut m, cluster, _) = manager(forward(Target::Pod("db-0".into()), IdleTimeout::Never));
        cluster.apply_pod("n", pod("db-0", "db", 100, true));
        m.activate("f").await.unwrap();
        wait(&m, "connected", connected_to("db-0")).await;
        cluster.delete_pod("n", "db-0");
        wait(&m, "pod gone", |s| {
            matches!(s, Some(RealState::Reconnecting(Reason::PodNotReady { .. })))
        })
        .await;
        cluster.apply_pod("n", pod("db-0", "db", 300, true));
        wait(&m, "pod back", connected_to("db-0")).await;
    }
}
