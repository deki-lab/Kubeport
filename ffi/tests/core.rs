//! Tests of the FFI bridge as Swift uses it.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use kubeport_ffi::{Core, CoreListener, ForwardInput, ForwardView, TargetKind};

#[derive(Clone, Default)]
struct Recorder(Arc<Mutex<Vec<ForwardView>>>);

impl CoreListener for Recorder {
    fn forwards_changed(&self, forwards: Vec<ForwardView>, _can_restore: bool) {
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
            "timed out waiting for: {what}; saw: {:?}",
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

/// Without a cluster: save, Active restored on relaunch, removal.
#[tokio::test(flavor = "current_thread")]
async fn saves_restores_and_removes() {
    let dir = temp_dir("session");
    let config = dir.join("config.toml");
    let path = Some(config.to_string_lossy().into_owned());

    let recorder = Recorder::default();
    let core = Core::new(path.clone(), Box::new(recorder.clone())).unwrap();
    assert!(
        std::fs::read_to_string(&config)
            .unwrap()
            .contains("[[forward]]"),
        "commented example created"
    );

    core.save_forward(None, input("a", "missing-context", 28001))
        .await
        .unwrap();
    core.save_forward(None, input("b", "missing-context", 28002))
        .await
        .unwrap();
    let duplicate = core
        .save_forward(None, input("a", "missing-context", 28003))
        .await;
    assert!(duplicate.is_err(), "duplicate name rejected");

    core.activate("a".into());
    recorder
        .wait("a Active in Error", |v| {
            v.iter()
                .any(|f| f.name == "a" && f.active && f.state.is_some())
        })
        .await;
    drop(core);
    tokio::time::sleep(Duration::from_millis(200)).await;

    let restored = Recorder::default();
    let core = Core::new(path, Box::new(restored.clone())).unwrap();
    restored
        .wait("a restored Active, b Inactive", |v| {
            v.len() == 2
                && v.iter().any(|f| f.name == "a" && f.active)
                && v.iter().any(|f| f.name == "b" && !f.active)
        })
        .await;

    core.remove_forward("b".into()).await.unwrap();
    restored.wait("b removed", |v| v.len() == 1).await;
    assert!(
        !std::fs::read_to_string(&config)
            .unwrap()
            .contains("name = \"b\"")
    );
    std::fs::remove_dir_all(dir).unwrap();
}

/// With docker-desktop and tests/fixtures/whoami.yaml applied:
/// `cargo test -p kubeport-ffi -- --ignored`
#[tokio::test(flavor = "current_thread")]
#[ignore]
async fn form_on_docker_desktop() {
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
        "ExternalName hidden"
    );
    assert!(
        targets.iter().any(|t| t.kind == TargetKind::Pod),
        "pods offered"
    );

    core.save_forward(None, input("w", "docker-desktop", 28080))
        .await
        .unwrap();
    core.activate("w".into());
    recorder
        .wait("w Connected", |v| {
            v.iter()
                .any(|f| matches!(f.state, Some(kubeport_ffi::RealStateView::Connected { .. })))
        })
        .await;

    // Editing an Active Forward restarts it with its new settings.
    core.save_forward(Some("w".into()), input("w2", "docker-desktop", 28081))
        .await
        .unwrap();
    recorder
        .wait("w2 Active on 28081", |v| {
            v.iter()
                .any(|f| f.name == "w2" && f.active && f.local_port == 28081)
        })
        .await;
    std::fs::remove_dir_all(dir).unwrap();
}
