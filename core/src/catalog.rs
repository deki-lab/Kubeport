//! Ce que propose le cluster au formulaire de création : namespaces, Services, Pods.
//! Appels `list` ponctuels, faits seulement quand le formulaire en a besoin.

use k8s_openapi::api::core::v1::{Namespace, Pod, Service};
use kube::api::ListParams;
use kube::{Api, Client};

use crate::config::Target;
use crate::resolve::service_backend;

/// Port proposé pour une Cible : numéro et nom éventuel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortChoice {
    pub port: u16,
    pub name: Option<String>,
}

/// Cible proposée : un Service forwardable ou un Pod en cours d'exécution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetChoice {
    pub target: Target,
    pub ports: Vec<PortChoice>,
}

pub async fn namespaces(client: Client) -> Result<Vec<String>, String> {
    let list = Api::<Namespace>::all(client)
        .list(&ListParams::default())
        .await
        .map_err(|e| e.to_string())?;
    let mut names: Vec<String> = list
        .items
        .into_iter()
        .filter_map(|n| n.metadata.name)
        .collect();
    names.sort();
    Ok(names)
}

/// Services forwardables (ni `ExternalName`, ni sans selector) puis Pods `Running`.
pub async fn targets(client: Client, namespace: &str) -> Result<Vec<TargetChoice>, String> {
    let services = Api::<Service>::namespaced(client.clone(), namespace)
        .list(&ListParams::default())
        .await
        .map_err(|e| e.to_string())?;
    let pods = Api::<Pod>::namespaced(client, namespace)
        .list(&ListParams::default().fields("status.phase=Running"))
        .await
        .map_err(|e| e.to_string())?;

    let mut choices: Vec<TargetChoice> = services.items.iter().filter_map(service_choice).collect();
    choices.sort_by(|a, b| target_name(a).cmp(target_name(b)));
    let mut pod_choices: Vec<TargetChoice> = pods.items.iter().filter_map(pod_choice).collect();
    pod_choices.sort_by(|a, b| target_name(a).cmp(target_name(b)));
    choices.extend(pod_choices);
    Ok(choices)
}

fn target_name(choice: &TargetChoice) -> &str {
    match &choice.target {
        Target::Service(n) | Target::Pod(n) => n,
    }
}

pub(crate) fn service_choice(service: &Service) -> Option<TargetChoice> {
    let name = service.metadata.name.clone()?;
    let ports: Vec<PortChoice> = service
        .spec
        .as_ref()?
        .ports
        .iter()
        .flatten()
        .filter_map(|p| {
            Some(PortChoice {
                port: u16::try_from(p.port).ok()?,
                name: p.name.clone(),
            })
        })
        .collect();
    // Même règle qu'à l'activation : un Service sans pods derrière n'est pas proposé.
    let first = ports.first()?;
    service_backend(service, first.port).ok()?;
    Some(TargetChoice {
        target: Target::Service(name),
        ports,
    })
}

pub(crate) fn pod_choice(pod: &Pod) -> Option<TargetChoice> {
    let name = pod.metadata.name.clone()?;
    let ports = pod
        .spec
        .as_ref()?
        .containers
        .iter()
        .flat_map(|c| c.ports.iter().flatten())
        .filter_map(|p| {
            Some(PortChoice {
                port: u16::try_from(p.container_port).ok()?,
                name: p.name.clone(),
            })
        })
        .collect();
    Some(TargetChoice {
        target: Target::Pod(name),
        ports,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use k8s_openapi::api::core::v1::{ServicePort, ServiceSpec};
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;

    fn service(name: &str, type_: Option<&str>, selector: bool) -> Service {
        Service {
            metadata: ObjectMeta {
                name: Some(name.into()),
                ..Default::default()
            },
            spec: Some(ServiceSpec {
                type_: type_.map(Into::into),
                selector: selector.then(|| [("app".to_string(), "x".to_string())].into()),
                ports: Some(vec![ServicePort {
                    port: 80,
                    name: Some("http".into()),
                    ..Default::default()
                }]),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn ne_propose_que_les_services_forwardables() {
        assert!(service_choice(&service("web", None, true)).is_some());
        assert!(service_choice(&service("ext", Some("ExternalName"), true)).is_none());
        assert!(service_choice(&service("manuel", None, false)).is_none());
        let web = service_choice(&service("web", None, true)).unwrap();
        assert_eq!(
            web.ports,
            [PortChoice {
                port: 80,
                name: Some("http".into())
            }]
        );
    }
}
