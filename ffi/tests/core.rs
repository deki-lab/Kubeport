//! Tests du pont FFI tel que Swift l'utilise.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use kubeport_ffi::{Core, CoreListener, ForwardInput, ForwardView, TargetKind};

#[derive(Clone, Default)]
struct Recorder(Arc<Mutex<Vec<ForwardView>>>);

impl CoreListener for Recorder {
    fn forwards_changed(&self, forwards: Vec<ForwardView>) {
        *self.0.lock().unwrap() = forwards;
    }
    fn notice(&self, _notice: kubeport_ffi::Notice) {}
}

impl Recorder {
    async fn wait(&self, what: &str, ok: impl Fn(&[ForwardView]) -> bool) {
        for _ in 0..100 {
            if ok(&self.0.lock().unwrap()) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!(
            "attente dépassée : {what} ; vu : {:?}",
            self.0.lock().unwrap()
        );
    }
}

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kubeport-ffi-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn input(name: &str, context: &str, local_port: u16) -> ForwardInput {
    ForwardInput {
        name: name.into(),
        context: context.into(),
        namespace: "kubeport-it".into(),
        target_kind: TargetKind::Service,
        target_name: "whoami".into(),
        remote_port: 8080,
        local_port,
        idle_timeout: kubeport_ffi::IdleTimeoutView::Inherit,
    }
}

/// Sans cluster : enregistrement, Actif restauré au relancement, suppression.
#[tokio::test(flavor = "current_thread")]
async fn enregistre_restaure_et_supprime() {
    let dir = temp_dir("session");
    let config = dir.join("config.toml");
    let path = Some(config.to_string_lossy().into_owned());

    let recorder = Recorder::default();
    let core = Core::new(path.clone(), Box::new(recorder.clone())).unwrap();
    assert!(
        std::fs::read_to_string(&config)
            .unwrap()
            .contains("[[forward]]"),
        "exemple commenté créé"
    );

    core.save_forward(None, input("a", "contexte-absent", 28001))
        .await
        .unwrap();
    core.save_forward(None, input("b", "contexte-absent", 28002))
        .await
        .unwrap();
    let doublon = core
        .save_forward(None, input("a", "contexte-absent", 28003))
        .await;
    assert!(doublon.is_err(), "nom en double refusé");

    core.activate("a".into());
    recorder
        .wait("a Actif en Erreur", |v| {
            v.iter()
                .any(|f| f.name == "a" && f.active && f.state.is_some())
        })
        .await;
    drop(core);
    tokio::time::sleep(Duration::from_millis(200)).await;

    let restored = Recorder::default();
    let core = Core::new(path, Box::new(restored.clone())).unwrap();
    restored
        .wait("a restauré Actif, b Inactif", |v| {
            v.len() == 2
                && v.iter().any(|f| f.name == "a" && f.active)
                && v.iter().any(|f| f.name == "b" && !f.active)
        })
        .await;

    core.remove_forward("b".into()).await.unwrap();
    restored.wait("b supprimé", |v| v.len() == 1).await;
    assert!(
        !std::fs::read_to_string(&config)
            .unwrap()
            .contains("name = \"b\"")
    );
    std::fs::remove_dir_all(dir).unwrap();
}

/// Avec docker-desktop et tests/fixtures/whoami.yaml appliqué :
/// `cargo test -p kubeport-ffi -- --ignored`
#[tokio::test(flavor = "current_thread")]
#[ignore]
async fn formulaire_sur_docker_desktop() {
    let dir = temp_dir("cluster");
    let path = Some(dir.join("config.toml").to_string_lossy().into_owned());
    let recorder = Recorder::default();
    let core = Core::new(path, Box::new(recorder.clone())).unwrap();

    assert!(
        core.contexts()
            .await
            .unwrap()
            .contains(&"docker-desktop".to_string())
    );
    let namespaces = core.namespaces("docker-desktop".into()).await.unwrap();
    assert!(namespaces.contains(&"kubeport-it".to_string()));
    let targets = core
        .targets("docker-desktop".into(), "kubeport-it".into())
        .await
        .unwrap();
    let whoami = targets
        .iter()
        .find(|t| t.kind == TargetKind::Service && t.name == "whoami")
        .unwrap();
    assert_eq!(whoami.ports[0].port, 8080);
    assert!(
        !targets.iter().any(|t| t.name == "external"),
        "ExternalName masqué"
    );
    assert!(
        targets.iter().any(|t| t.kind == TargetKind::Pod),
        "pods proposés"
    );

    core.save_forward(None, input("w", "docker-desktop", 28080))
        .await
        .unwrap();
    core.activate("w".into());
    recorder
        .wait("w Connecté", |v| {
            v.iter()
                .any(|f| matches!(f.state, Some(kubeport_ffi::RealStateView::Connected { .. })))
        })
        .await;

    // Modifier un Forward Actif le relance avec ses nouveaux réglages.
    core.save_forward(Some("w".into()), input("w2", "docker-desktop", 28081))
        .await
        .unwrap();
    recorder
        .wait("w2 Actif sur 28081", |v| {
            v.iter()
                .any(|f| f.name == "w2" && f.active && f.local_port == 28081)
        })
        .await;
    std::fs::remove_dir_all(dir).unwrap();
}
