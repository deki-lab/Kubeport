//! Running an Active Forward: local port, Resolved pod tracking, one stream per connection.
//! It only talks to the cluster through the `Cluster` seam.

use std::collections::BTreeMap;
use std::io::ErrorKind;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use futures::StreamExt;
use k8s_openapi::api::core::v1::Pod;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinSet;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::cluster::{Cluster, Fault, PodEvent, PodFilter};
use crate::config::{Forward, Target};
use crate::resolve::{PortRef, Resolved, resolve, service_backend};
use crate::state::{RealState, Reason};

pub(crate) type Report = Arc<dyn Fn(RealState) + Send + Sync>;

/// Open connections per Forward; beyond this, new connections are closed at once.
const MAX_CONNECTIONS: usize = 256;

/// Pause after a failed `accept` (out of file descriptors): no busy loop.
const ACCEPT_BACKOFF: Duration = Duration::from_millis(100);

/// The local port of an Active Forward, on 127.0.0.1 and, when the Mac has IPv6, on [::1].
///
/// The Manager binds it on activation and keeps it until the Forward becomes Inactive:
/// reconnections and Errors never release it, so no other local process can take it
/// while the user's clients still point at it. Binding [::1] too means `localhost`
/// reaches the Forward whichever address a client tries first.
pub(crate) struct LocalPort {
    v4: TcpListener,
    v6: Option<TcpListener>,
}

impl LocalPort {
    pub(crate) async fn bind(port: u16) -> Result<Self, Reason> {
        let v4 = TcpListener::bind((Ipv4Addr::LOCALHOST, port))
            .await
            .map_err(|e| port_error(port, &e))?;
        let v6 = match TcpListener::bind((Ipv6Addr::LOCALHOST, port)).await {
            Ok(l) => Some(l),
            Err(e) if e.kind() == ErrorKind::AddrInUse => return Err(Reason::PortInUse { port }),
            // No IPv6 loopback on this Mac: 127.0.0.1 alone.
            Err(_) => None,
        };
        Ok(Self { v4, v6 })
    }

    async fn accept(&self) -> std::io::Result<TcpStream> {
        let accepted = match &self.v6 {
            None => self.v4.accept().await,
            Some(v6) => tokio::select! {
                r = self.v4.accept() => r,
                r = v6.accept() => r,
            },
        };
        accepted.map(|(socket, _)| socket)
    }
}

fn port_error(port: u16, e: &std::io::Error) -> Reason {
    if e.kind() == ErrorKind::AddrInUse {
        Reason::PortInUse { port }
    } else {
        Reason::PortUnavailable {
            port,
            detail: e.to_string(),
        }
    }
}

/// Idle timeout of an Active Forward: goes Inactive after `timeout` with no open connection.
pub(crate) struct Idle {
    pub timeout: Option<Duration>,
    /// Publishes the current deadline (`None`: a connection is open, or no timeout).
    pub deadline: Arc<dyn Fn(Option<SystemTime>) + Send + Sync>,
    /// Timeout elapsed: the Manager makes the Forward Inactive.
    pub expired: Arc<dyn Fn() + Send + Sync>,
}

impl Idle {
    fn arm(&self, timeout: Duration) -> Instant {
        (self.deadline)(Some(SystemTime::now() + timeout));
        Instant::now() + timeout
    }
}

/// Keeps an Active Forward running until `cancel`. After an Error it keeps the local
/// port and closes every new connection, until the user acts.
pub(crate) async fn run<C: Cluster>(
    forward: Forward,
    cluster: C,
    port: Arc<LocalPort>,
    report: Report,
    idle: Idle,
    cancel: CancellationToken,
) {
    tokio::select! {
        biased;
        () = cancel.cancelled() => {}
        () = async {
            serve(&forward, cluster, &port, &report, &idle).await;
            refuse(&port).await;
        } => {}
    }
}

/// Holds the local port of a Forward in Error until `cancel`, closing every connection.
pub(crate) async fn hold(port: Arc<LocalPort>, cancel: CancellationToken) {
    tokio::select! {
        biased;
        () = cancel.cancelled() => {}
        () = refuse(&port) => {}
    }
}

async fn refuse(port: &LocalPort) {
    loop {
        match port.accept().await {
            Ok(socket) => drop(socket),
            Err(_) => tokio::time::sleep(ACCEPT_BACKOFF).await,
        }
    }
}

async fn serve<C: Cluster>(
    forward: &Forward,
    cluster: C,
    port: &LocalPort,
    report: &Report,
    idle: &Idle,
) {
    let (filter, pod_port, missing) = match &forward.target {
        Target::Service(name) => {
            let Some(backend) = service_with_retry(&cluster, name, forward, report).await else {
                return;
            };
            (
                PodFilter::Labels(backend.selector),
                backend.port,
                Reason::NoReadyPod,
            )
        }
        Target::Pod(name) => (
            PodFilter::Name(name.clone()),
            PortRef::Number(forward.remote_port),
            Reason::PodNotReady { pod: name.clone() },
        ),
    };
    let pods = cluster.pods(&forward.namespace, filter);

    report(RealState::Reconnecting(Reason::SearchingPod));
    let (resolved_tx, resolved_rx) = watch::channel(None::<Resolved>);
    let (fatal_tx, mut fatal_rx) = mpsc::channel::<Reason>(1);

    tokio::select! {
        () = track_pods(pods, pod_port, missing, resolved_tx, report) => {}
        () = accept_loop(port, &cluster, &forward.namespace, resolved_rx, fatal_tx, idle) => {}
        Some(reason) = fatal_rx.recv() => report(RealState::Error(reason)),
    }
}

/// Reads the Service; retries while the API is unreachable, gives up on an Error.
async fn service_with_retry<C: Cluster>(
    cluster: &C,
    name: &str,
    forward: &Forward,
    report: &Report,
) -> Option<crate::resolve::ServiceBackend> {
    let mut delay = Duration::from_secs(1);
    loop {
        match cluster.service(&forward.namespace, name).await {
            Ok(svc) => {
                return match service_backend(&svc, forward.remote_port) {
                    Ok(b) => Some(b),
                    Err(reason) => {
                        report(RealState::Error(reason));
                        None
                    }
                };
            }
            Err(fault) => match fault {
                Fault::Fatal(reason) => {
                    let reason = match reason {
                        Reason::NotFound { .. } => Reason::ServiceNotFound {
                            namespace: forward.namespace.clone(),
                            service: name.to_owned(),
                        },
                        other => other,
                    };
                    report(RealState::Error(reason));
                    return None;
                }
                Fault::Transient(reason) => {
                    report(RealState::Reconnecting(reason));
                    tokio::time::sleep(delay).await;
                    delay = (delay * 2).min(Duration::from_secs(30));
                }
            },
        }
    }
}

/// Follows the Target's pods and publishes the Resolved pod on every change.
async fn track_pods(
    mut events: futures::stream::BoxStream<'static, PodEvent>,
    port: PortRef,
    missing: Reason,
    resolved_tx: watch::Sender<Option<Resolved>>,
    report: &Report,
) {
    let mut store: BTreeMap<String, Pod> = BTreeMap::new();

    while let Some(event) = events.next().await {
        match event {
            PodEvent::Reset(pods) => store = pods.into_iter().map(|p| (pod_name(&p), p)).collect(),
            PodEvent::Applied(pod) => {
                store.insert(pod_name(&pod), pod);
            }
            PodEvent::Deleted(pod) => {
                store.remove(&pod_name(&pod));
            }
            PodEvent::Failed(Fault::Fatal(reason)) => return report(RealState::Error(reason)),
            PodEvent::Failed(Fault::Transient(reason)) => {
                report(RealState::Reconnecting(reason));
                continue;
            }
        }

        let next = resolve(resolved_tx.borrow().as_ref(), store.values(), &port);
        match &next {
            Some(r) => report(RealState::Connected { pod: r.pod.clone() }),
            None => report(RealState::Reconnecting(missing.clone())),
        }
        resolved_tx.send_if_modified(|current| {
            let changed = *current != next;
            *current = next;
            changed
        });
    }
}

fn pod_name(pod: &Pod) -> String {
    pod.metadata.name.clone().unwrap_or_default()
}

/// Accepts local connections; each opens its own stream to the current Resolved pod.
async fn accept_loop<C: Cluster>(
    port: &LocalPort,
    cluster: &C,
    namespace: &str,
    resolved: watch::Receiver<Option<Resolved>>,
    fatal: mpsc::Sender<Reason>,
    idle: &Idle,
) {
    // Dropping the JoinSet cuts every connection when the Forward stops.
    let mut connections = JoinSet::new();
    // Deactivation deadline: armed while no connection is open.
    let mut idle_at = idle.timeout.map(|t| idle.arm(t));
    loop {
        // The future is created even when the branch is disabled: never unwrap here.
        let far = Instant::now() + Duration::from_secs(365 * 24 * 3600);
        tokio::select! {
            () = tokio::time::sleep_until(idle_at.unwrap_or(far)), if idle_at.is_some() => {
                (idle.expired)();
                return;
            }
            accepted = port.accept() => {
                let Ok(socket) = accepted else {
                    tokio::time::sleep(ACCEPT_BACKOFF).await;
                    continue;
                };
                if connections.len() >= MAX_CONNECTIONS {
                    tracing::warn!("{MAX_CONNECTIONS} open connections: refusing a new one");
                    continue;
                }
                let Some(target) = resolved.borrow().clone() else {
                    // No Ready pod: refuse the connection rather than keep it waiting.
                    continue;
                };
                if idle_at.take().is_some() {
                    (idle.deadline)(None);
                }
                let cluster = cluster.clone();
                let namespace = namespace.to_owned();
                let fatal = fatal.clone();
                connections.spawn(async move {
                    match pipe(&cluster, &namespace, &target, socket).await {
                        Ok(()) => {}
                        Err(Fault::Fatal(reason)) => {
                            tracing::warn!(pod = %target.pod, "connection refused: {reason}");
                            let _ = fatal.try_send(reason);
                        }
                        Err(Fault::Transient(reason)) => {
                            tracing::warn!(pod = %target.pod, "connection closed: {reason}");
                        }
                    }
                });
            }
            Some(_) = connections.join_next(), if !connections.is_empty() => {
                if connections.is_empty() {
                    idle_at = idle.timeout.map(|t| idle.arm(t));
                }
            }
        }
    }
}

async fn pipe<C: Cluster>(
    cluster: &C,
    namespace: &str,
    target: &Resolved,
    mut socket: TcpStream,
) -> Result<(), Fault> {
    let _ = socket.set_nodelay(true);
    let mut upstream = cluster.connect(namespace, &target.pod, target.port).await?;
    // A broken copy is a closed connection, not a Forward failure.
    let _ = tokio::io::copy_bidirectional(&mut socket, &mut upstream).await;
    Ok(())
}
