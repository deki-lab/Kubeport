//! The real cluster, through kube-rs: one client per Context, `exec` auth plugins run
//! with the login shell's `PATH`, errors sorted into `Fault`s.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use futures::StreamExt;
use futures::stream::BoxStream;
use k8s_openapi::api::core::v1::{Pod, Service};
use kube::api::Api;
use kube::client::AuthError;
use kube::config::{KubeConfigOptions, Kubeconfig};
use kube::runtime::{WatchStreamExt, watcher};
use kube::{Client, Config as KubeConfig};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use super::{Cluster, Connector, Fault, PodEvent, PodFilter, PodStream};
use crate::shell_env::ShellEnv;
use crate::state::Reason;

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

/// Hands out a kube-rs client per Context, cached until `forget`.
pub struct KubeConnector {
    env: KubeEnv,
    clients: HashMap<String, Client>,
}

impl KubeConnector {
    pub fn new(env: KubeEnv) -> Self {
        Self {
            env,
            clients: HashMap::new(),
        }
    }

    /// Cached client for the Context; also used by the form's one-off listings.
    pub async fn client(
        &mut self,
        kubeconfig: Option<&Path>,
        context: &str,
    ) -> Result<Client, Reason> {
        if let Some(client) = self.clients.get(context) {
            return Ok(client.clone());
        }
        let mut file = self.read_kubeconfig(kubeconfig)?;
        if !file.contexts.iter().any(|c| c.name == context) {
            return Err(Reason::ContextMissing {
                context: context.to_owned(),
            });
        }
        if let Some(path) = &self.env.exec_path {
            inject_exec_path(&mut file, path);
        }
        let options = KubeConfigOptions {
            context: Some(context.to_owned()),
            ..Default::default()
        };
        let config = KubeConfig::from_custom_kubeconfig(file, &options)
            .await
            .map_err(|e| Reason::Kubeconfig {
                detail: format!("{context}: {e}"),
            })?;
        let client = Client::try_from(config).map_err(|e| Reason::Kubeconfig {
            detail: format!("{context}: {}", describe(&e)),
        })?;
        self.clients.insert(context.to_owned(), client.clone());
        Ok(client)
    }

    /// Priority order: the config's `kubeconfig` setting, the login shell's `KUBECONFIG`,
    /// then standard behaviour (the process `KUBECONFIG`, `~/.kube/config`).
    fn read_kubeconfig(&self, setting: Option<&Path>) -> Result<Kubeconfig, Reason> {
        let paths: Vec<PathBuf> = match setting {
            Some(path) => vec![path.to_owned()],
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

impl Connector for KubeConnector {
    type Cluster = KubeCluster;

    async fn cluster(
        &mut self,
        kubeconfig: Option<&Path>,
        context: &str,
    ) -> Result<KubeCluster, Reason> {
        Ok(KubeCluster {
            client: self.client(kubeconfig, context).await?,
        })
    }

    fn contexts(&self, kubeconfig: Option<&Path>) -> Result<Vec<String>, Reason> {
        let mut names: Vec<String> = self
            .read_kubeconfig(kubeconfig)?
            .contexts
            .into_iter()
            .map(|c| c.name)
            .collect();
        names.sort();
        Ok(names)
    }

    fn forget(&mut self) {
        self.clients.clear();
    }
}

#[derive(Clone)]
pub struct KubeCluster {
    client: Client,
}

impl Cluster for KubeCluster {
    async fn service(&self, namespace: &str, name: &str) -> Result<Service, Fault> {
        Api::<Service>::namespaced(self.client.clone(), namespace)
            .get(name)
            .await
            .map_err(|e| classify(&e))
    }

    fn pods(&self, namespace: &str, filter: PodFilter) -> BoxStream<'static, PodEvent> {
        let config = match filter {
            PodFilter::Labels(selector) => watcher::Config::default().labels(&selector),
            PodFilter::Name(name) => {
                watcher::Config::default().fields(&format!("metadata.name={name}"))
            }
        };
        let api = Api::<Pod>::namespaced(self.client.clone(), namespace);
        // The watcher's Init / InitApply / InitDone become one Reset with the full list.
        let mut buffer: Option<Vec<Pod>> = None;
        watcher(api, config)
            .default_backoff()
            .filter_map(move |event| {
                let out = match event {
                    Ok(watcher::Event::Init) => {
                        buffer = Some(Vec::new());
                        None
                    }
                    Ok(watcher::Event::InitApply(pod)) => {
                        if let Some(b) = buffer.as_mut() {
                            b.push(pod);
                        }
                        None
                    }
                    Ok(watcher::Event::InitDone) => {
                        Some(PodEvent::Reset(buffer.take().unwrap_or_default()))
                    }
                    Ok(watcher::Event::Apply(pod)) => Some(PodEvent::Applied(pod)),
                    Ok(watcher::Event::Delete(pod)) => Some(PodEvent::Deleted(pod)),
                    Err(e) => Some(PodEvent::Failed(classify_watch(&e))),
                };
                futures::future::ready(out)
            })
            .boxed()
    }

    async fn connect(&self, namespace: &str, pod: &str, port: u16) -> Result<PodStream, Fault> {
        let mut forwarder = Api::<Pod>::namespaced(self.client.clone(), namespace)
            .portforward(pod, &[port])
            .await
            .map_err(|e| match e {
                // The pod went away between two watch events: the watch moves the
                // Resolved pod, only this connection fails.
                kube::Error::Api(status) if status.code == 404 => {
                    Fault::Transient(Reason::PodNotReady {
                        pod: pod.to_owned(),
                    })
                }
                e => classify(&e),
            })?;
        let stream = forwarder.take_stream(port).ok_or_else(|| {
            Fault::Transient(Reason::ApiUnreachable {
                detail: "port-forward stream missing".into(),
            })
        })?;
        Ok(Box::new(Forwarded {
            stream,
            _forwarder: forwarder,
        }))
    }
}

/// The port's stream, keeping its port-forward connection alive as long as it is used.
struct Forwarded {
    stream: tokio::io::DuplexStream,
    _forwarder: kube::api::Portforwarder,
}

impl AsyncRead for Forwarded {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.stream).poll_read(cx, buf)
    }
}

impl AsyncWrite for Forwarded {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::pin::Pin::new(&mut self.stream).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.stream).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}

fn classify(err: &kube::Error) -> Fault {
    match err {
        kube::Error::Api(status) => classify_status(status.code, &status.message),
        kube::Error::Auth(e) => Fault::Fatal(Reason::AuthFailed {
            detail: describe_auth(e),
        }),
        other => Fault::Transient(Reason::ApiUnreachable {
            detail: describe(other),
        }),
    }
}

fn classify_status(code: u16, message: &str) -> Fault {
    let detail = message.to_owned();
    match code {
        401 => Fault::Fatal(Reason::AuthFailed { detail }),
        403 => Fault::Fatal(Reason::AccessDenied { detail }),
        404 => Fault::Fatal(Reason::NotFound { detail }),
        // Timeout, conflict, expired watch, rate limit: retrying helps.
        408 | 409 | 410 | 429 => Fault::Transient(Reason::ApiError { code, detail }),
        // Any other client error stays the same on every retry.
        400..=499 => Fault::Fatal(Reason::ApiError { code, detail }),
        _ => Fault::Transient(Reason::ApiError { code, detail }),
    }
}

/// Text of a kube-rs error that is safe to show in the panel, a notification or the CLI.
/// An auth error found anywhere in the chain (a token refresh fails inside the client's
/// middleware) is described by `describe_auth` instead of the whole chain.
fn describe(err: &kube::Error) -> String {
    if let kube::Error::Auth(e) = err {
        return describe_auth(e);
    }
    let mut source = std::error::Error::source(err);
    while let Some(e) = source {
        if let Some(auth) = e.downcast_ref::<AuthError>() {
            return describe_auth(auth);
        }
        source = e.source();
    }
    err.to_string()
}

/// kube-rs puts an `exec` plugin's arguments, environment and output in its error text;
/// they can hold secrets (`--client-secret`, tokens). Keep the exit status only.
fn describe_auth(err: &AuthError) -> String {
    match err {
        AuthError::AuthExecRun { out, .. } => {
            format!("auth exec plugin failed ({})", out.status)
        }
        AuthError::AuthExecParse(_) => "auth exec plugin returned an unreadable credential".into(),
        AuthError::AuthExec(_) => "auth exec plugin failed".into(),
        other => other.to_string(),
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

    #[test]
    fn exec_plugin_errors_never_show_args_env_or_output() {
        use std::os::unix::process::ExitStatusExt;
        let out = std::process::Output {
            status: std::process::ExitStatus::from_raw(256),
            stdout: b"SECRET_STDOUT".to_vec(),
            stderr: b"SECRET_STDERR".to_vec(),
        };
        let run = AuthError::AuthExecRun {
            cmd: r#"TOKEN="SECRET_ENV" "kubelogin" "--client-secret=SECRET_ARG""#.into(),
            out: Box::new(out),
        };
        let leaked = |text: &str| text.contains("SECRET");

        let direct = describe(&kube::Error::Auth(run));
        assert!(!leaked(&direct), "{direct}");
        assert!(direct.contains("exit status: 1"), "{direct}");

        // A failed token refresh comes back wrapped by the client's middleware.
        let run = AuthError::AuthExecRun {
            cmd: "SECRET_ARG".into(),
            out: Box::new(std::process::Output {
                status: std::process::ExitStatus::from_raw(256),
                stdout: b"SECRET_STDOUT".to_vec(),
                stderr: Vec::new(),
            }),
        };
        let wrapped = describe(&kube::Error::Service(Box::new(run)));
        assert!(!leaked(&wrapped), "{wrapped}");

        let exec = describe(&kube::Error::Auth(AuthError::AuthExec(
            "Executing SECRET_ARG failed".into(),
        )));
        assert!(!leaked(&exec), "{exec}");
    }

    #[test]
    fn permanent_client_errors_are_fatal() {
        assert!(matches!(
            classify_status(422, "invalid"),
            Fault::Fatal(Reason::ApiError { code: 422, .. })
        ));
        for code in [408, 409, 410, 429] {
            assert!(
                matches!(classify_status(code, "retry"), Fault::Transient(_)),
                "{code}"
            );
        }
    }

    #[test]
    fn sorts_api_errors_into_faults() {
        assert!(matches!(
            classify_status(403, "no"),
            Fault::Fatal(Reason::AccessDenied { .. })
        ));
        assert!(matches!(
            classify_status(401, "no"),
            Fault::Fatal(Reason::AuthFailed { .. })
        ));
        assert!(matches!(
            classify_status(503, "busy"),
            Fault::Transient(Reason::ApiError { code: 503, .. })
        ));
    }
}
