//! What the user does with Kubeport, behind one interface: turning Forwards on and off,
//! the master switch, editing and removing Forwards, settings, reloading the config file,
//! and the Idle timeout. The Session owns the Manager, `config.toml` and `state.toml`, so
//! adapters (the UniFFI bridge, the CLI) only translate calls and notices.
//!
//! `state.toml` sits next to the config and holds the Active Forwards (restored on the
//! next launch) and the set the master switch will bring back. The config stays
//! hand-editable without being rewritten on every click.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, watch};

use crate::config::{Config, ConfigError, Forward, minutes, write_atomic};
use crate::manager::{KubeEnv, Manager, ManagerError, Statuses};
use crate::state::Reason;

/// Short message for the user, produced by a Session operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
    /// `rival` was turned off: `by` uses the same local port (Competing Forwards).
    RivalDeactivated {
        rival: String,
        by: String,
    },
    /// The Idle timeout of `name` ran out: it is now Inactive.
    IdleDeactivated {
        name: String,
        minutes: u32,
    },
    UnknownForward {
        name: String,
    },
    ConfigNotReloaded {
        detail: String,
    },
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SavedState {
    #[serde(default)]
    active: Vec<String>,
    /// Forwards the master switch turns back on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    remembered: Vec<String>,
}

pub struct Session {
    manager: Manager,
    config_path: PathBuf,
    /// `None`: nothing is read from or written to `state.toml` (development CLI).
    state_path: Option<PathBuf>,
    expired: mpsc::UnboundedReceiver<String>,
    remembered: Vec<String>,
    saved: SavedState,
}

impl Session {
    /// Builds a Session on an already loaded config (see `Config::load_or_create`).
    /// Nothing is activated until `restore`.
    pub fn new(config: Config, config_path: PathBuf, env: KubeEnv) -> Self {
        let state_path = Some(config_path.with_file_name("state.toml"));
        let mut manager = Manager::new(config, env);
        let expired = manager.take_expired().expect("fresh Manager");
        Self {
            manager,
            config_path,
            state_path,
            expired,
            remembered: Vec::new(),
            saved: SavedState::default(),
        }
    }

    /// No `state.toml`: nothing restored, nothing saved.
    pub fn ephemeral(mut self) -> Self {
        self.state_path = None;
        self
    }

    /// Reactivates the Forwards that were Active at the last shutdown.
    pub async fn restore(&mut self) {
        let Some(path) = &self.state_path else { return };
        let saved = load_state(path);
        self.remembered = self.known(saved.remembered.clone());
        for name in self.known(saved.active.clone()) {
            let _ = self.manager.activate(&name).await;
        }
        self.saved = saved;
        self.persist();
    }

    pub fn config(&self) -> &Config {
        self.manager.config()
    }

    pub fn statuses(&self) -> watch::Receiver<Statuses> {
        self.manager.subscribe()
    }

    /// True when the master switch can bring Forwards back on.
    pub fn can_restore(&self) -> bool {
        !self.remembered.is_empty()
    }

    // MARK: Desired state

    /// Turns a Forward on. Turning one on by hand starts a new set: the master switch
    /// forgets what it remembered.
    pub async fn activate(&mut self, name: &str) -> Vec<Notice> {
        self.remembered.clear();
        let notices = self.activate_one(name).await;
        self.persist();
        notices
    }

    pub fn deactivate(&mut self, name: &str) {
        self.manager.deactivate(name);
        self.persist();
    }

    /// Master switch. Off: every Forward off, the set remembered. On: that set back on.
    pub async fn set_all(&mut self, on: bool) -> Vec<Notice> {
        let mut notices = Vec::new();
        if on {
            let remembered = std::mem::take(&mut self.remembered);
            for name in self.known(remembered) {
                notices.extend(self.activate_one(&name).await);
            }
        } else {
            let active = self.manager.active();
            if !active.is_empty() {
                self.remembered = active;
            }
            self.manager.deactivate_all();
        }
        self.persist();
        notices
    }

    /// After a wake-up or a network change: same Active Forwards, fresh connections.
    pub async fn reconnect_all(&mut self) {
        self.manager.reconnect_all().await;
    }

    /// Waits until an Idle timeout runs out, turns that Forward off and says so.
    /// Meant for a `select!` loop next to the adapter's own commands.
    pub async fn next_expiry(&mut self) -> Option<Notice> {
        let name = self.expired.recv().await?;
        let minutes = self
            .config()
            .get(&name)
            .and_then(|f| self.config().idle_timeout(f))
            .map(minutes)
            .unwrap_or_default();
        self.manager.deactivate(&name);
        self.persist();
        Some(Notice::IdleDeactivated { name, minutes })
    }

    // MARK: Editing

    /// Adds a Forward, or replaces `previous` (which may be renamed). An Active Forward
    /// stays Active with its new settings; other Forwards are not touched.
    pub async fn save_forward(
        &mut self,
        previous: Option<&str>,
        forward: Forward,
    ) -> Result<Vec<Notice>, ConfigError> {
        let was_active = previous.is_some_and(|p| self.is_active(p));
        let name = forward.name.clone();
        let mut config = self.config().clone();
        config.upsert(previous, forward)?;
        self.apply(config).await?;
        if let Some(previous) = previous.filter(|p| *p != name) {
            for n in &mut self.remembered {
                if n == previous {
                    n.clone_from(&name);
                }
            }
        }
        let notices = if was_active {
            self.activate_one(&name).await
        } else {
            Vec::new()
        };
        self.persist();
        Ok(notices)
    }

    pub async fn remove_forward(&mut self, name: &str) -> Result<(), ConfigError> {
        let mut config = self.config().clone();
        config.remove(name);
        self.apply(config).await?;
        self.remembered.retain(|n| n != name);
        self.persist();
        Ok(())
    }

    /// Forced kubeconfig; `None` goes back to the login shell's, then `~/.kube/config`.
    pub async fn set_kubeconfig(&mut self, path: Option<PathBuf>) -> Result<(), ConfigError> {
        let mut config = self.config().clone();
        config.kubeconfig = path.filter(|p| !p.as_os_str().is_empty());
        self.apply(config).await?;
        self.persist();
        Ok(())
    }

    /// Default Idle timeout in minutes; `None` or 0: never.
    pub async fn set_default_idle_timeout(
        &mut self,
        minutes: Option<u32>,
    ) -> Result<(), ConfigError> {
        let mut config = self.config().clone();
        config.idle_timeout_minutes = minutes.filter(|m| *m > 0);
        self.apply(config).await?;
        self.persist();
        Ok(())
    }

    /// Rereads `config.toml` after a hand edit. Modified Forwards become Inactive.
    pub async fn reload(&mut self) -> Option<Notice> {
        match Config::load(&self.config_path) {
            Ok(config) => {
                self.manager.replace_config(config).await;
                let remembered = std::mem::take(&mut self.remembered);
                self.remembered = self.known(remembered);
                self.persist();
                None
            }
            Err(e) => Some(Notice::ConfigNotReloaded {
                detail: e.to_string(),
            }),
        }
    }

    // MARK: Cluster lookups for the form

    pub fn contexts(&self) -> Result<Vec<String>, Reason> {
        self.manager.contexts()
    }

    pub async fn client(&mut self, context: &str) -> Result<kube::Client, Reason> {
        self.manager.client(context).await
    }

    // MARK: Internals

    async fn activate_one(&mut self, name: &str) -> Vec<Notice> {
        match self.manager.activate(name).await {
            Ok(rivals) => rivals
                .into_iter()
                .map(|rival| Notice::RivalDeactivated {
                    rival,
                    by: name.to_owned(),
                })
                .collect(),
            Err(ManagerError::UnknownForward(name)) => vec![Notice::UnknownForward { name }],
        }
    }

    /// Saves the config, then hands it to the Manager (which restarts only what changed).
    async fn apply(&mut self, config: Config) -> Result<(), ConfigError> {
        config.save(&self.config_path)?;
        self.manager.replace_config(config).await;
        Ok(())
    }

    fn is_active(&self, name: &str) -> bool {
        self.manager.active().iter().any(|n| n == name)
    }

    fn known(&self, names: Vec<String>) -> Vec<String> {
        names
            .into_iter()
            .filter(|n| self.config().get(n).is_some())
            .collect()
    }

    /// Writes `state.toml` when the Active set or the remembered set changed.
    fn persist(&mut self) {
        let Some(path) = self.state_path.clone() else {
            return;
        };
        let state = SavedState {
            active: self.manager.active(),
            remembered: self.remembered.clone(),
        };
        if state == self.saved {
            return;
        }
        match toml::to_string(&state) {
            Ok(text) => {
                if let Err(e) = write_atomic(&path, &text) {
                    tracing::warn!("state.toml: {e}");
                    return;
                }
                self.saved = state;
            }
            Err(e) => tracing::warn!("state.toml: {e}"),
        }
    }
}

fn load_state(path: &Path) -> SavedState {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::RealState;

    /// Unreadable kubeconfig: every activation ends in Error, without a cluster.
    fn env() -> KubeEnv {
        KubeEnv {
            kubeconfig: vec!["/nonexistent/kubeconfig".into()],
            exec_path: None,
        }
    }

    fn forward(name: &str, port: u16) -> Forward {
        Config::parse(&format!(
            "[[forward]]\nname = \"{name}\"\ncontext = \"c\"\nnamespace = \"n\"\nservice = \"s\"\nremote_port = 80\nlocal_port = {port}\n"
        ))
        .unwrap()
        .forwards
        .remove(0)
    }

    struct Dir(PathBuf);
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn dir(tag: &str) -> Dir {
        let d = std::env::temp_dir().join(format!("kubeport-session-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Dir(d)
    }

    async fn session(dir: &Dir, forwards: &[(&str, u16)]) -> Session {
        let path = dir.0.join("config.toml");
        let config = Config::load_or_create(&path).unwrap();
        let mut s = Session::new(config, path, env());
        for (name, port) in forwards {
            s.save_forward(None, forward(name, *port)).await.unwrap();
        }
        s
    }

    async fn reopen(dir: &Dir) -> Session {
        let path = dir.0.join("config.toml");
        let mut s = Session::new(Config::load(&path).unwrap(), path, env());
        s.restore().await;
        s
    }

    fn active(s: &Session) -> Vec<String> {
        s.manager.active()
    }

    #[tokio::test(flavor = "current_thread")]
    async fn creates_the_config_file_from_the_template() {
        let d = dir("template");
        let _ = session(&d, &[]).await;
        let text = std::fs::read_to_string(d.0.join("config.toml")).unwrap();
        assert!(text.contains("# [[forward]]"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn restores_active_forwards_on_next_launch() {
        let d = dir("restore");
        let mut s = session(&d, &[("a", 1), ("b", 2)]).await;
        s.activate("a").await;
        drop(s);
        let s = reopen(&d).await;
        assert_eq!(active(&s), ["a"]);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn editing_an_active_forward_keeps_it_active_and_reports_rivals() {
        let d = dir("edit");
        let mut s = session(&d, &[("a", 1), ("b", 2)]).await;
        s.activate("a").await;
        s.activate("b").await;
        // Move "a" onto b's local port: b is turned off, and the user is told.
        let notices = s.save_forward(Some("a"), forward("a", 2)).await.unwrap();
        assert_eq!(active(&s), ["a"]);
        assert_eq!(
            notices,
            [Notice::RivalDeactivated {
                rival: "b".into(),
                by: "a".into()
            }]
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn saving_one_forward_leaves_the_others_running() {
        let d = dir("targeted");
        let mut s = session(&d, &[("a", 1), ("b", 2)]).await;
        s.activate("a").await;
        s.activate("b").await;
        let before = s.statuses().borrow()["b"].clone();
        s.save_forward(Some("a"), forward("a", 3)).await.unwrap();
        assert_eq!(active(&s), ["a", "b"]);
        assert_eq!(s.statuses().borrow()["b"], before);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn master_switch_turns_everything_off_and_the_same_set_back_on() {
        let d = dir("master");
        let mut s = session(&d, &[("a", 1), ("b", 2), ("c", 3)]).await;
        s.activate("a").await;
        s.activate("c").await;
        s.set_all(false).await;
        assert!(active(&s).is_empty());
        assert!(s.can_restore());
        // Survives a restart of the app.
        drop(s);
        let mut s = reopen(&d).await;
        assert!(s.can_restore());
        s.set_all(true).await;
        assert_eq!(active(&s), ["a", "c"]);
        assert!(!s.can_restore());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn renaming_keeps_the_forward_in_the_remembered_set() {
        let d = dir("rename");
        let mut s = session(&d, &[("a", 1)]).await;
        s.activate("a").await;
        s.set_all(false).await;
        s.save_forward(Some("a"), forward("renamed", 1))
            .await
            .unwrap();
        s.set_all(true).await;
        assert_eq!(active(&s), ["renamed"]);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn activating_by_hand_forgets_the_remembered_set() {
        let d = dir("forget");
        let mut s = session(&d, &[("a", 1), ("b", 2)]).await;
        s.activate("a").await;
        s.set_all(false).await;
        s.activate("b").await;
        assert!(!s.can_restore());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn removing_a_forward_drops_it_everywhere() {
        let d = dir("remove");
        let mut s = session(&d, &[("a", 1), ("b", 2)]).await;
        s.activate("a").await;
        s.remove_forward("a").await.unwrap();
        assert!(active(&s).is_empty());
        assert!(s.config().get("a").is_none());
        assert!(!s.statuses().borrow().contains_key("a"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn default_idle_timeout_zero_means_never() {
        let d = dir("idle");
        let mut s = session(&d, &[]).await;
        s.set_default_idle_timeout(Some(0)).await.unwrap();
        assert_eq!(s.config().idle_timeout_minutes, None);
        s.set_default_idle_timeout(Some(60)).await.unwrap();
        assert_eq!(s.config().idle_timeout_minutes, Some(60));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn reload_reports_an_unreadable_file() {
        let d = dir("reload");
        let mut s = session(&d, &[("a", 1)]).await;
        std::fs::write(d.0.join("config.toml"), "not = [valid").unwrap();
        assert!(matches!(
            s.reload().await,
            Some(Notice::ConfigNotReloaded { .. })
        ));
        assert!(s.config().get("a").is_some(), "previous config kept");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn activation_without_cluster_is_active_in_error() {
        let d = dir("error");
        let mut s = session(&d, &[("a", 1)]).await;
        s.activate("a").await;
        let status = s.statuses().borrow()["a"].clone();
        assert!(status.active);
        assert!(matches!(status.real, Some(RealState::Error(_))));
    }
}
