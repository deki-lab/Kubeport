//! Resolved pod selection and port translation: pure logic, no network.

use std::collections::BTreeMap;

use k8s_openapi::api::core::v1::{Pod, Service};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;

use crate::state::Reason;

/// Port targeted in the pod: a number, or a container port name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortRef {
    Number(u16),
    Name(String),
}

/// Resolved pod and the numeric port to open on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub pod: String,
    pub port: u16,
}

/// What to watch for a Service Target: its selector and the pod port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceBackend {
    pub selector: String,
    pub port: PortRef,
}

/// Reads the Service selector and translates `remote_port` (Service port) into the pod port.
///
/// An error here needs human action: the Service cannot be forwarded.
pub fn service_backend(service: &Service, remote_port: u16) -> Result<ServiceBackend, Reason> {
    let spec = service
        .spec
        .as_ref()
        .ok_or(Reason::ServiceWithoutSelector)?;
    if spec.type_.as_deref() == Some("ExternalName") {
        return Err(Reason::ExternalNameService);
    }
    let selector = spec
        .selector
        .as_ref()
        .filter(|s| !s.is_empty())
        .ok_or(Reason::ServiceWithoutSelector)?;
    let service_port = spec
        .ports
        .iter()
        .flatten()
        .find(|p| p.port == i32::from(remote_port))
        .ok_or(Reason::PortNotExposed { port: remote_port })?;
    let port = match &service_port.target_port {
        None => PortRef::Number(remote_port),
        Some(IntOrString::Int(n)) => {
            PortRef::Number(u16::try_from(*n).map_err(|_| Reason::InvalidTargetPort { value: *n })?)
        }
        Some(IntOrString::String(name)) => PortRef::Name(name.clone()),
    };
    Ok(ServiceBackend {
        selector: selector_string(selector),
        port,
    })
}

fn selector_string(selector: &BTreeMap<String, String>) -> String {
    selector
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(",")
}

/// A pod is usable if it is Ready and not being deleted (`Terminating`).
pub fn is_ready(pod: &Pod) -> bool {
    if pod.metadata.deletion_timestamp.is_some() {
        return false;
    }
    pod.status
        .as_ref()
        .and_then(|s| s.conditions.as_ref())
        .into_iter()
        .flatten()
        .any(|c| c.type_ == "Ready" && c.status == "True")
}

/// Translates a `PortRef` into a port number for this pod (named ports included).
pub fn pod_port(pod: &Pod, port: &PortRef) -> Option<u16> {
    match port {
        PortRef::Number(n) => Some(*n),
        PortRef::Name(name) => pod
            .spec
            .as_ref()?
            .containers
            .iter()
            .flat_map(|c| c.ports.iter().flatten())
            .find(|p| p.name.as_deref() == Some(name))
            .and_then(|p| u16::try_from(p.container_port).ok()),
    }
}

/// Picks the Resolved pod among `pods`.
///
/// The current Resolved pod is kept as long as it stays Ready; otherwise the oldest
/// Ready pod wins (creation date, then name as a tie-breaker).
pub fn resolve<'a>(
    current: Option<&Resolved>,
    pods: impl IntoIterator<Item = &'a Pod>,
    port: &PortRef,
) -> Option<Resolved> {
    let mut best: Option<(&Pod, u16)> = None;
    for pod in pods {
        if !is_ready(pod) {
            continue;
        }
        let Some(number) = pod_port(pod, port) else {
            continue;
        };
        let name = pod.metadata.name.as_deref().unwrap_or_default();
        if current.is_some_and(|c| c.pod == name) {
            return Some(Resolved {
                pod: name.to_owned(),
                port: number,
            });
        }
        if best.is_none_or(|(b, _)| age_key(pod) < age_key(b)) {
            best = Some((pod, number));
        }
    }
    best.map(|(pod, port)| Resolved {
        pod: pod.metadata.name.clone().unwrap_or_default(),
        port,
    })
}

fn age_key(pod: &Pod) -> (Option<k8s_openapi::jiff::Timestamp>, &str) {
    (
        pod.metadata.creation_timestamp.as_ref().map(|t| t.0),
        pod.metadata.name.as_deref().unwrap_or_default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use k8s_openapi::api::core::v1::{
        Container, ContainerPort, PodCondition, PodSpec, PodStatus, ServicePort, ServiceSpec,
    };
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::{ObjectMeta, Time};

    fn pod(name: &str, created_s: i64, ready: bool) -> Pod {
        Pod {
            metadata: ObjectMeta {
                name: Some(name.into()),
                creation_timestamp: Some(Time(
                    k8s_openapi::jiff::Timestamp::from_second(created_s).unwrap(),
                )),
                ..Default::default()
            },
            spec: Some(PodSpec {
                containers: vec![Container {
                    name: "app".into(),
                    ports: Some(vec![ContainerPort {
                        name: Some("http".into()),
                        container_port: 8080,
                        ..Default::default()
                    }]),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            status: Some(PodStatus {
                conditions: Some(vec![PodCondition {
                    type_: "Ready".into(),
                    status: if ready { "True" } else { "False" }.into(),
                    ..Default::default()
                }]),
                ..Default::default()
            }),
        }
    }

    fn terminating(mut p: Pod) -> Pod {
        p.metadata.deletion_timestamp = p.metadata.creation_timestamp.clone();
        p
    }

    fn service(type_: Option<&str>, selector: &[(&str, &str)], ports: Vec<ServicePort>) -> Service {
        Service {
            spec: Some(ServiceSpec {
                type_: type_.map(Into::into),
                selector: (!selector.is_empty()).then(|| {
                    selector
                        .iter()
                        .map(|(k, v)| (k.to_string(), v.to_string()))
                        .collect()
                }),
                ports: Some(ports),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    fn sport(port: i32, target: Option<IntOrString>) -> ServicePort {
        ServicePort {
            port,
            target_port: target,
            ..Default::default()
        }
    }

    #[test]
    fn picks_the_oldest_ready_pod() {
        let pods = [
            pod("b", 200, true),
            pod("a", 100, false),
            pod("c", 150, true),
        ];
        let r = resolve(None, &pods, &PortRef::Number(80)).unwrap();
        assert_eq!(
            r,
            Resolved {
                pod: "c".into(),
                port: 80
            }
        );
    }

    #[test]
    fn keeps_the_resolved_pod_while_it_is_ready() {
        let pods = [pod("old", 100, true), pod("current", 200, true)];
        let current = Resolved {
            pod: "current".into(),
            port: 80,
        };
        let r = resolve(Some(&current), &pods, &PortRef::Number(80)).unwrap();
        assert_eq!(r.pod, "current");
    }

    #[test]
    fn switches_when_the_resolved_pod_turns_terminating() {
        let pods = [
            terminating(pod("current", 100, true)),
            pod("new", 300, true),
        ];
        let current = Resolved {
            pod: "current".into(),
            port: 80,
        };
        let r = resolve(Some(&current), &pods, &PortRef::Number(80)).unwrap();
        assert_eq!(r.pod, "new");
    }

    #[test]
    fn no_ready_pod_gives_none() {
        let pods = [pod("a", 100, false), terminating(pod("b", 100, true))];
        assert_eq!(resolve(None, &pods, &PortRef::Number(80)), None);
    }

    #[test]
    fn translates_a_named_port() {
        let pods = [pod("a", 100, true)];
        let r = resolve(None, &pods, &PortRef::Name("http".into())).unwrap();
        assert_eq!(r.port, 8080);
        assert_eq!(resolve(None, &pods, &PortRef::Name("grpc".into())), None);
    }

    #[test]
    fn service_translates_target_port() {
        let svc = service(
            None,
            &[("app", "web"), ("tier", "front")],
            vec![
                sport(80, Some(IntOrString::String("http".into()))),
                sport(443, Some(IntOrString::Int(8443))),
                sport(9000, None),
            ],
        );
        let b = service_backend(&svc, 80).unwrap();
        assert_eq!(b.selector, "app=web,tier=front");
        assert_eq!(b.port, PortRef::Name("http".into()));
        assert_eq!(
            service_backend(&svc, 443).unwrap().port,
            PortRef::Number(8443)
        );
        assert_eq!(
            service_backend(&svc, 9000).unwrap().port,
            PortRef::Number(9000)
        );
    }

    #[test]
    fn service_without_pods_is_rejected() {
        let external = service(Some("ExternalName"), &[("a", "b")], vec![sport(80, None)]);
        assert_eq!(
            service_backend(&external, 80).unwrap_err(),
            Reason::ExternalNameService
        );
        let no_selector = service(None, &[], vec![sport(80, None)]);
        assert_eq!(
            service_backend(&no_selector, 80).unwrap_err(),
            Reason::ServiceWithoutSelector
        );
    }

    #[test]
    fn port_missing_from_the_service_is_rejected() {
        let svc = service(None, &[("a", "b")], vec![sport(80, None)]);
        assert_eq!(
            service_backend(&svc, 5432).unwrap_err(),
            Reason::PortNotExposed { port: 5432 }
        );
    }
}
