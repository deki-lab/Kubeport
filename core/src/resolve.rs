//! Choix du Pod résolu et traduction du port : logique pure, sans réseau.

use std::collections::BTreeMap;

use k8s_openapi::api::core::v1::{Pod, Service};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;

/// Port visé dans le pod : un numéro, ou un nom de port de conteneur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortRef {
    Number(u16),
    Name(String),
}

/// Pod résolu et port numérique à ouvrir dessus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub pod: String,
    pub port: u16,
}

/// Ce qu'il faut surveiller pour une Cible Service : son selector et le port du pod.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceBackend {
    pub selector: String,
    pub port: PortRef,
}

/// Lit le selector du Service et traduit `remote_port` (port du Service) en port du pod.
///
/// Une erreur ici demande une action humaine : le Service ne peut pas être forwardé.
pub fn service_backend(service: &Service, remote_port: u16) -> Result<ServiceBackend, String> {
    let spec = service.spec.as_ref().ok_or("Service sans spec")?;
    if spec.type_.as_deref() == Some("ExternalName") {
        return Err("Service ExternalName : aucun pod derrière".into());
    }
    let selector = spec
        .selector
        .as_ref()
        .filter(|s| !s.is_empty())
        .ok_or("Service sans selector : aucun pod derrière")?;
    let service_port = spec
        .ports
        .iter()
        .flatten()
        .find(|p| p.port == i32::from(remote_port))
        .ok_or_else(|| format!("le Service n'expose pas le port {remote_port}"))?;
    let port = match &service_port.target_port {
        None => PortRef::Number(remote_port),
        Some(IntOrString::Int(n)) => {
            PortRef::Number(u16::try_from(*n).map_err(|_| format!("targetPort {n} invalide"))?)
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

/// Un pod est utilisable s'il est Ready et pas en cours de suppression (`Terminating`).
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

/// Traduit un `PortRef` en numéro de port pour ce pod (ports nommés compris).
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

/// Choisit le Pod résolu parmi `pods`.
///
/// Le Pod résolu actuel est conservé tant qu'il reste Ready ; sinon on prend le plus
/// ancien pod Ready (date de création, puis nom pour départager).
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
    fn prend_le_plus_ancien_pod_ready() {
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
    fn garde_le_pod_resolu_tant_qu_il_est_ready() {
        let pods = [pod("vieux", 100, true), pod("courant", 200, true)];
        let current = Resolved {
            pod: "courant".into(),
            port: 80,
        };
        let r = resolve(Some(&current), &pods, &PortRef::Number(80)).unwrap();
        assert_eq!(r.pod, "courant");
    }

    #[test]
    fn bascule_quand_le_pod_resolu_passe_terminating() {
        let pods = [
            terminating(pod("courant", 100, true)),
            pod("neuf", 300, true),
        ];
        let current = Resolved {
            pod: "courant".into(),
            port: 80,
        };
        let r = resolve(Some(&current), &pods, &PortRef::Number(80)).unwrap();
        assert_eq!(r.pod, "neuf");
    }

    #[test]
    fn aucun_pod_ready_donne_none() {
        let pods = [pod("a", 100, false), terminating(pod("b", 100, true))];
        assert_eq!(resolve(None, &pods, &PortRef::Number(80)), None);
    }

    #[test]
    fn traduit_un_port_nomme() {
        let pods = [pod("a", 100, true)];
        let r = resolve(None, &pods, &PortRef::Name("http".into())).unwrap();
        assert_eq!(r.port, 8080);
        assert_eq!(resolve(None, &pods, &PortRef::Name("grpc".into())), None);
    }

    #[test]
    fn service_traduit_target_port() {
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
    fn service_sans_pods_est_refuse() {
        let external = service(Some("ExternalName"), &[("a", "b")], vec![sport(80, None)]);
        assert!(
            service_backend(&external, 80)
                .unwrap_err()
                .contains("ExternalName")
        );
        let no_selector = service(None, &[], vec![sport(80, None)]);
        assert!(
            service_backend(&no_selector, 80)
                .unwrap_err()
                .contains("sans selector")
        );
    }

    #[test]
    fn port_absent_du_service_est_refuse() {
        let svc = service(None, &[("a", "b")], vec![sport(80, None)]);
        assert!(service_backend(&svc, 5432).unwrap_err().contains("5432"));
    }
}
