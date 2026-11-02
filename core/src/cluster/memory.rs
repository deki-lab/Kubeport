//! An in-memory cluster for tests: Services and pods are set by the test, the pod watch
//! is a channel, and a connection is a `tokio::io::duplex` whose far end writes the name
//! of the pod that served it, then echoes.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::{Arc, Mutex};

use futures::StreamExt;
use futures::stream::BoxStream;
use k8s_openapi::api::core::v1::{Pod, PodCondition, PodStatus, Service};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{ObjectMeta, Time};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::broadcast;

use super::{Cluster, Connector, Fault, PodEvent, PodFilter, PodStream};
use crate::state::Reason;

#[derive(Default)]
struct Inner {
    services: HashMap<(String, String), Service>,
    pods: BTreeMap<(String, String), Pod>,
    /// Faults returned by the next `connect` calls, in order.
    connect_faults: Vec<Fault>,
    /// Pods that served each connection, in order.
    connections: Vec<String>,
}

#[derive(Clone)]
pub struct MemoryCluster {
    inner: Arc<Mutex<Inner>>,
    events: broadcast::Sender<(String, PodEvent)>,
}

impl MemoryCluster {
    pub fn new() -> Self {
        Self {
            inner: Arc::default(),
            events: broadcast::channel(64).0,
        }
    }

    pub fn set_service(&self, namespace: &str, service: Service) {
        let name = service.metadata.name.clone().unwrap_or_default();
        self.inner
            .lock()
            .unwrap()
            .services
            .insert((namespace.into(), name), service);
    }

    /// Adds or updates a pod, as the API server would after a change.
    pub fn apply_pod(&self, namespace: &str, pod: Pod) {
        let name = pod.metadata.name.clone().unwrap_or_default();
        self.inner
            .lock()
            .unwrap()
            .pods
            .insert((namespace.into(), name), pod.clone());
        let _ = self.events.send((namespace.into(), PodEvent::Applied(pod)));
    }

    pub fn delete_pod(&self, namespace: &str, name: &str) {
        let removed = self
            .inner
            .lock()
            .unwrap()
            .pods
            .remove(&(namespace.into(), name.into()));
        if let Some(pod) = removed {
            let _ = self.events.send((namespace.into(), PodEvent::Deleted(pod)));
        }
    }

    /// The pod watch fails, as when the API server drops it.
    pub fn fail_watch(&self, namespace: &str, fault: Fault) {
        let _ = self
            .events
            .send((namespace.into(), PodEvent::Failed(fault)));
    }

    pub fn fail_next_connect(&self, fault: Fault) {
        self.inner.lock().unwrap().connect_faults.push(fault);
    }

    pub fn connections(&self) -> Vec<String> {
        self.inner.lock().unwrap().connections.clone()
    }
}

fn matches(pod: &Pod, filter: &PodFilter) -> bool {
    match filter {
        PodFilter::Name(name) => pod.metadata.name.as_deref() == Some(name),
        PodFilter::Labels(selector) => {
            let labels = pod.metadata.labels.clone().unwrap_or_default();
            selector.split(',').all(|pair| {
                let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
                labels.get(k).map(String::as_str) == Some(v)
            })
        }
    }
}

impl Cluster for MemoryCluster {
    async fn service(&self, namespace: &str, name: &str) -> Result<Service, Fault> {
        self.inner
            .lock()
            .unwrap()
            .services
            .get(&(namespace.into(), name.into()))
            .cloned()
            .ok_or_else(|| {
                Fault::Fatal(Reason::NotFound {
                    detail: format!("services \"{name}\" not found"),
                })
            })
    }

    fn pods(&self, namespace: &str, filter: PodFilter) -> BoxStream<'static, PodEvent> {
        let namespace = namespace.to_owned();
        // Subscribe before the snapshot: no change can slip between them.
        let receiver = self.events.subscribe();
        let snapshot: Vec<Pod> = self
            .inner
            .lock()
            .unwrap()
            .pods
            .iter()
            .filter(|((ns, _), pod)| *ns == namespace && matches(pod, &filter))
            .map(|(_, pod)| pod.clone())
            .collect();
        let live = tokio_stream_from(receiver).filter_map(move |(ns, event)| {
            let keep = ns == namespace
                && match &event {
                    PodEvent::Applied(p) | PodEvent::Deleted(p) => matches(p, &filter),
                    PodEvent::Reset(_) | PodEvent::Failed(_) => true,
                };
            futures::future::ready(keep.then_some(event))
        });
        futures::stream::once(futures::future::ready(PodEvent::Reset(snapshot)))
            .chain(live)
            .boxed()
    }

    async fn connect(&self, _namespace: &str, pod: &str, _port: u16) -> Result<PodStream, Fault> {
        {
            let mut inner = self.inner.lock().unwrap();
            if !inner.connect_faults.is_empty() {
                return Err(inner.connect_faults.remove(0));
            }
            inner.connections.push(pod.to_owned());
        }
        let (near, mut far) = tokio::io::duplex(4096);
        let pod = pod.to_owned();
        tokio::spawn(async move {
            let _ = far.write_all(format!("{pod}\n").as_bytes()).await;
            let mut buf = [0u8; 1024];
            while let Ok(n) = far.read(&mut buf).await {
                if n == 0 || far.write_all(&buf[..n]).await.is_err() {
                    break;
                }
            }
        });
        Ok(Box::new(near))
    }
}

fn tokio_stream_from(
    receiver: broadcast::Receiver<(String, PodEvent)>,
) -> BoxStream<'static, (String, PodEvent)> {
    futures::stream::unfold(receiver, |mut rx| async move {
        loop {
            match rx.recv().await {
                Ok(item) => return Some((item, rx)),
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    })
    .boxed()
}

/// One `MemoryCluster` per Context name; an unknown Context is missing from the kubeconfig.
#[derive(Default)]
pub struct MemoryConnector {
    pub clusters: HashMap<String, MemoryCluster>,
}

impl Connector for MemoryConnector {
    type Cluster = MemoryCluster;

    async fn cluster(
        &mut self,
        _kubeconfig: Option<&Path>,
        context: &str,
    ) -> Result<MemoryCluster, Reason> {
        self.clusters
            .get(context)
            .cloned()
            .ok_or_else(|| Reason::ContextMissing {
                context: context.to_owned(),
            })
    }

    fn contexts(&self, _kubeconfig: Option<&Path>) -> Result<Vec<String>, Reason> {
        let mut names: Vec<String> = self.clusters.keys().cloned().collect();
        names.sort();
        Ok(names)
    }

    fn forget(&mut self) {}
}

/// A pod with labels `app=<app>`, created at `created_s`, Ready or not.
pub fn pod(name: &str, app: &str, created_s: i64, ready: bool) -> Pod {
    Pod {
        metadata: ObjectMeta {
            name: Some(name.into()),
            labels: Some([("app".to_string(), app.to_string())].into()),
            creation_timestamp: Some(Time(
                k8s_openapi::jiff::Timestamp::from_second(created_s).unwrap(),
            )),
            ..Default::default()
        },
        status: Some(PodStatus {
            conditions: Some(vec![PodCondition {
                type_: "Ready".into(),
                status: if ready { "True" } else { "False" }.into(),
                ..Default::default()
            }]),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// A Service selecting `app=<app>`, port 80 to the pods' port 8080.
pub fn service(name: &str, app: &str) -> Service {
    use k8s_openapi::api::core::v1::{ServicePort, ServiceSpec};
    use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
    Service {
        metadata: ObjectMeta {
            name: Some(name.into()),
            ..Default::default()
        },
        spec: Some(ServiceSpec {
            selector: Some([("app".to_string(), app.to_string())].into()),
            ports: Some(vec![ServicePort {
                port: 80,
                target_port: Some(IntOrString::Int(8080)),
                ..Default::default()
            }]),
            ..Default::default()
        }),
        ..Default::default()
    }
}
