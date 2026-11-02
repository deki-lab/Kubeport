//! Exécution d'un Forward Actif : écoute locale, suivi du Pod résolu, un flux par connexion.

use std::collections::BTreeMap;
use std::net::Ipv4Addr;
use std::pin::pin;
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use k8s_openapi::api::core::v1::{Pod, Service};
use kube::runtime::{WatchStreamExt, watcher};
use kube::{Api, Client};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::config::{Forward, Target};
use crate::resolve::{PortRef, Resolved, resolve, service_backend};
use crate::state::{RealState, Reason};

pub(crate) type Report = Arc<dyn Fn(RealState) + Send + Sync>;

/// Erreur classée : `Fatal` demande une action humaine, `Transient` se répare seule.
enum Fault {
    Fatal(Reason),
    Transient(Reason),
}

fn classify(err: &kube::Error) -> Fault {
    match err {
        kube::Error::Api(status) => classify_status(status.code, &status.message),
        kube::Error::Auth(e) => Fault::Fatal(Reason::AuthFailed {
            detail: e.to_string(),
        }),
        other => Fault::Transient(Reason::ApiUnreachable {
            detail: other.to_string(),
        }),
    }
}

fn classify_status(code: u16, message: &str) -> Fault {
    let detail = message.to_owned();
    match code {
        401 => Fault::Fatal(Reason::AuthFailed { detail }),
        403 => Fault::Fatal(Reason::AccessDenied { detail }),
        404 => Fault::Fatal(Reason::NotFound { detail }),
        _ => Fault::Transient(Reason::ApiError { code, detail }),
    }
}

fn classify_watch(err: &watcher::Error) -> Fault {
    match err {
        watcher::Error::InitialListFailed(e)
        | watcher::Error::WatchStartFailed(e)
        | watcher::Error::WatchFailed(e) => classify(e),
        watcher::Error::WatchError(status) => classify_status(status.code, &status.message),
        watcher::Error::NoResourceVersion => Fault::Transient(Reason::ApiUnreachable {
            detail: err.to_string(),
        }),
    }
}

/// Fait vivre un Forward Actif jusqu'à `cancel`, ou jusqu'à une Erreur.
pub(crate) async fn run(
    forward: Forward,
    client: Client,
    report: Report,
    cancel: CancellationToken,
) {
    tokio::select! {
        biased;
        () = cancel.cancelled() => {}
        () = serve(&forward, client, &report) => {}
    }
}

async fn serve(forward: &Forward, client: Client, report: &Report) {
    let listener = match TcpListener::bind((Ipv4Addr::LOCALHOST, forward.local_port)).await {
        Ok(l) => l,
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            return report(RealState::Error(Reason::PortInUse {
                port: forward.local_port,
            }));
        }
        Err(e) => {
            return report(RealState::Error(Reason::PortUnavailable {
                port: forward.local_port,
                detail: e.to_string(),
            }));
        }
    };

    let pods: Api<Pod> = Api::namespaced(client.clone(), &forward.namespace);
    let (watch_config, port, missing) = match &forward.target {
        Target::Service(name) => {
            let services: Api<Service> = Api::namespaced(client, &forward.namespace);
            let Some(backend) = service_with_retry(&services, name, forward, report).await else {
                return;
            };
            (
                watcher::Config::default().labels(&backend.selector),
                backend.port,
                Reason::NoReadyPod,
            )
        }
        Target::Pod(name) => (
            watcher::Config::default().fields(&format!("metadata.name={name}")),
            PortRef::Number(forward.remote_port),
            Reason::PodNotReady { pod: name.clone() },
        ),
    };

    report(RealState::Reconnecting(Reason::SearchingPod));
    let (resolved_tx, resolved_rx) = watch::channel(None::<Resolved>);
    let (fatal_tx, mut fatal_rx) = mpsc::channel::<Reason>(1);

    tokio::select! {
        () = track_pods(pods.clone(), watch_config, port, missing, resolved_tx, report) => {}
        () = accept_loop(listener, pods, resolved_rx, fatal_tx) => {}
        Some(reason) = fatal_rx.recv() => report(RealState::Error(reason)),
    }
}

/// Lit le Service ; réessaie tant que l'API est injoignable, abandonne sur une Erreur.
async fn service_with_retry(
    services: &Api<Service>,
    name: &str,
    forward: &Forward,
    report: &Report,
) -> Option<crate::resolve::ServiceBackend> {
    let mut delay = Duration::from_secs(1);
    loop {
        match services.get(name).await {
            Ok(svc) => {
                return match service_backend(&svc, forward.remote_port) {
                    Ok(b) => Some(b),
                    Err(reason) => {
                        report(RealState::Error(reason));
                        None
                    }
                };
            }
            Err(e) => match classify(&e) {
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

/// Surveille les pods de la Cible et publie le Pod résolu à chaque changement.
async fn track_pods(
    pods: Api<Pod>,
    config: watcher::Config,
    port: PortRef,
    missing: Reason,
    resolved_tx: watch::Sender<Option<Resolved>>,
    report: &Report,
) {
    let mut events = pin!(watcher(pods, config).default_backoff());
    let mut store: BTreeMap<String, Pod> = BTreeMap::new();
    let mut init: Option<BTreeMap<String, Pod>> = None;

    while let Some(event) = events.next().await {
        match event {
            Ok(watcher::Event::Init) => {
                init = Some(BTreeMap::new());
                continue;
            }
            Ok(watcher::Event::InitApply(pod)) => {
                if let Some(buf) = init.as_mut() {
                    buf.insert(pod_name(&pod), pod);
                }
                continue;
            }
            Ok(watcher::Event::InitDone) => store = init.take().unwrap_or_default(),
            Ok(watcher::Event::Apply(pod)) => {
                store.insert(pod_name(&pod), pod);
            }
            Ok(watcher::Event::Delete(pod)) => {
                store.remove(&pod_name(&pod));
            }
            Err(e) => match classify_watch(&e) {
                Fault::Fatal(reason) => return report(RealState::Error(reason)),
                Fault::Transient(reason) => {
                    report(RealState::Reconnecting(reason));
                    continue;
                }
            },
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

/// Accepte les connexions locales ; chacune ouvre son propre flux vers le Pod résolu du moment.
async fn accept_loop(
    listener: TcpListener,
    pods: Api<Pod>,
    resolved: watch::Receiver<Option<Resolved>>,
    fatal: mpsc::Sender<Reason>,
) {
    // Abandonner le JoinSet coupe toutes les connexions quand le Forward s'arrête.
    let mut connections = JoinSet::new();
    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let Ok((socket, _)) = accepted else { continue };
                let Some(target) = resolved.borrow().clone() else {
                    // Aucun pod Ready : on refuse la connexion plutôt que de la faire attendre.
                    continue;
                };
                let pods = pods.clone();
                let fatal = fatal.clone();
                connections.spawn(async move {
                    if let Err(e) = pipe(&pods, &target, socket).await {
                        tracing::warn!(pod = %target.pod, "connection closed: {e}");
                        if let Some(kube_err) = e.downcast_ref::<kube::Error>()
                            && let Fault::Fatal(reason) = classify(kube_err)
                        {
                            let _ = fatal.try_send(reason);
                        }
                    }
                });
            }
            Some(_) = connections.join_next(), if !connections.is_empty() => {}
        }
    }
}

type BoxError = Box<dyn std::error::Error + Send + Sync>;

async fn pipe(pods: &Api<Pod>, target: &Resolved, mut socket: TcpStream) -> Result<(), BoxError> {
    socket.set_nodelay(true)?;
    let mut forwarder = pods.portforward(&target.pod, &[target.port]).await?;
    let mut upstream = forwarder
        .take_stream(target.port)
        .ok_or("port-forward stream missing")?;
    tokio::io::copy_bidirectional(&mut socket, &mut upstream).await?;
    drop(upstream);
    forwarder.join().await?;
    Ok(())
}
