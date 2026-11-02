//! Environnement du shell de login de l'utilisateur.
//!
//! Une app lancée au login n'hérite pas du shell : ni `PATH` (où vivent `aws`,
//! `gke-gcloud-auth-plugin`, `kubelogin`…), ni `KUBECONFIG`. On les lit une fois au
//! démarrage en lançant le shell de login.

use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const MARKER: &str = "__KUBEPORT_ENV__";
const TIMEOUT: Duration = Duration::from_secs(5);

/// Variables utiles lues dans le shell de login.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ShellEnv {
    pub path: Option<String>,
    pub kubeconfig: Vec<PathBuf>,
}

/// Lance `$SHELL -ilc` et lit `PATH` et `KUBECONFIG`. Renvoie un environnement vide
/// si le shell échoue ou dépasse 5 s : l'app retombe alors sur l'environnement du process.
pub fn login_shell_env() -> ShellEnv {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| {
        let zsh = std::path::Path::new("/bin/zsh");
        if zsh.exists() { "/bin/zsh" } else { "/bin/sh" }.into()
    });
    let script = format!(r#"printf '{MARKER}%s{MARKER}%s{MARKER}' "$PATH" "${{KUBECONFIG-}}""#);
    let Ok(mut child) = Command::new(shell)
        .args(["-ilc", &script])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return ShellEnv::default();
    };

    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() < TIMEOUT => {
                std::thread::sleep(Duration::from_millis(20))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return ShellEnv::default();
            }
        }
    }
    let mut output = String::new();
    if let Some(mut stdout) = child.stdout.take() {
        let _ = stdout.read_to_string(&mut output);
    }
    parse(&output)
}

/// Extrait les valeurs entre marqueurs : les rc files peuvent écrire n'importe quoi autour.
fn parse(output: &str) -> ShellEnv {
    let mut parts = output.split(MARKER).skip(1);
    let (Some(path), Some(kubeconfig)) = (parts.next(), parts.next()) else {
        return ShellEnv::default();
    };
    ShellEnv {
        path: (!path.is_empty()).then(|| path.to_owned()),
        kubeconfig: std::env::split_paths(kubeconfig)
            .filter(|p| !p.as_os_str().is_empty())
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignore_le_bruit_des_rc_files() {
        let out = format!(
            "Bienvenue !\n{MARKER}/opt/homebrew/bin:/usr/bin{MARKER}/a/config:/b/config{MARKER}"
        );
        let env = parse(&out);
        assert_eq!(env.path.as_deref(), Some("/opt/homebrew/bin:/usr/bin"));
        assert_eq!(
            env.kubeconfig,
            [PathBuf::from("/a/config"), PathBuf::from("/b/config")]
        );
    }

    #[test]
    fn kubeconfig_vide_et_sortie_illisible() {
        let env = parse(&format!("{MARKER}/usr/bin{MARKER}{MARKER}"));
        assert!(env.kubeconfig.is_empty());
        assert_eq!(parse("rien"), ShellEnv::default());
    }

    #[test]
    fn lit_le_vrai_shell_de_login() {
        let env = login_shell_env();
        assert!(env.path.is_some_and(|p| p.contains("/usr/bin")));
    }
}
