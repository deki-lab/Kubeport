//! Forwards Active at shutdown, to reactivate them on the next launch.
//! Stored in `state.toml`, next to the config: the config stays hand-editable
//! without being rewritten on every click.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
struct Session {
    #[serde(default)]
    active: Vec<String>,
}

pub fn path_next_to(config_path: &Path) -> PathBuf {
    config_path.with_file_name("state.toml")
}

/// Names of the saved Active Forwards; empty if the file is missing or unreadable.
pub fn load_active(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| toml::from_str::<Session>(&text).ok())
        .map(|s| s.active)
        .unwrap_or_default()
}

pub fn save_active(path: &Path, active: &[String]) -> std::io::Result<()> {
    let session = Session {
        active: active.to_vec(),
    };
    let text = toml::to_string(&session).map_err(std::io::Error::other)?;
    crate::config::write_atomic(path, &text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_missing_file() {
        let dir = std::env::temp_dir().join(format!("kubeport-session-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.toml");
        assert!(load_active(&path).is_empty());
        save_active(&path, &["a".into(), "b".into()]).unwrap();
        assert_eq!(load_active(&path), ["a", "b"]);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
