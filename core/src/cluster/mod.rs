//! The seam between Forwards and a Kubernetes cluster.
//!
//! A running Forward needs three things from a cluster: read a Service, follow the pods
//! of its Target, and open a stream to a pod's port. `Cluster` is that interface, with
//! errors already sorted into what heals on its own and what needs a human. `Connector`
//! hands out a `Cluster` per Context.
//!
//! Two adapters: `kube` (the real cluster, through kube-rs) and, in tests, `memory`
//! (pods and streams held in memory), so the Forward runtime is tested without a cluster.

pub mod kube;
#[cfg(test)]
pub(crate) mod memory;

use std::future::Future;
use std::path::Path;

use futures::stream::BoxStream;
use k8s_openapi::api::core::v1::{Pod, Service};
use tokio::io::{AsyncRead, AsyncWrite};

use crate::state::Reason;

/// A cluster error, already sorted: `Fatal` needs human action, `Transient` heals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fault {
    Fatal(Reason),
    Transient(Reason),
}

/// Which pods a Forward follows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PodFilter {
    /// The pods matching a Service selector (`k=v,k2=v2`).
    Labels(String),
    /// One pod, by name (it may come back under the same name, as with a StatefulSet).
    Name(String),
}

/// What happened to the followed pods.
#[derive(Debug, Clone)]
pub enum PodEvent {
    /// The full current list (on start, and after each reconnection of the watch).
    Reset(Vec<Pod>),
    Applied(Pod),
    Deleted(Pod),
    /// The watch failed; a `Transient` failure retries on its own.
    Failed(Fault),
}

/// A byte stream to a pod's port.
pub trait PodIo: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> PodIo for T {}

pub type PodStream = Box<dyn PodIo>;

/// One Context of a cluster, as seen by a running Forward.
pub trait Cluster: Clone + Send + Sync + 'static {
    fn service(
        &self,
        namespace: &str,
        name: &str,
    ) -> impl Future<Output = Result<Service, Fault>> + Send;

    /// Follows the pods matching `filter`, forever; backoff on errors is the adapter's job.
    fn pods(&self, namespace: &str, filter: PodFilter) -> BoxStream<'static, PodEvent>;

    fn connect(
        &self,
        namespace: &str,
        pod: &str,
        port: u16,
    ) -> impl Future<Output = Result<PodStream, Fault>> + Send;
}

/// Hands out a `Cluster` per Context, reading the kubeconfig the config points to.
pub trait Connector: Send + 'static {
    type Cluster: Cluster;

    fn cluster(
        &mut self,
        kubeconfig: Option<&Path>,
        context: &str,
    ) -> impl Future<Output = Result<Self::Cluster, Reason>> + Send;

    /// Names of the Contexts, sorted.
    fn contexts(&self, kubeconfig: Option<&Path>) -> Result<Vec<String>, Reason>;

    /// Drops cached connections (new kubeconfig, wake-up, network change).
    fn forget(&mut self);
}
