//! Environment of the user's login shell.
//!
//! An app launched at login does not inherit from the shell: neither `PATH` (where `aws`,
//! `gke-gcloud-auth-plugin`, `kubelogin`… live) nor `KUBECONFIG`. We read them once at
//! startup by running the login shell.

use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const MARKER: &str = "__KUBEPORT_ENV__";
const TIMEOUT: Duration = Duration::from_secs(5);

/// Useful variables read from the login shell.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ShellEnv {
    pub path: Option<String>,
    pub kubeconfig: Vec<PathBuf>,
}

/// Runs `$SHELL -ilc` and reads `PATH` and `KUBECONFIG`. Returns an empty environment
/// if the shell fails or takes over 5 s: the app then falls back to the process environment.
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

/// Extracts the values between markers: rc files may print anything around them.
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
    fn ignores_rc_file_noise() {
        let out = format!(
            "Welcome!\n{MARKER}/opt/homebrew/bin:/usr/bin{MARKER}/a/config:/b/config{MARKER}"
        );
        let env = parse(&out);
        assert_eq!(env.path.as_deref(), Some("/opt/homebrew/bin:/usr/bin"));
        assert_eq!(
            env.kubeconfig,
            [PathBuf::from("/a/config"), PathBuf::from("/b/config")]
        );
    }

    #[test]
    fn empty_kubeconfig_and_unreadable_output() {
        let env = parse(&format!("{MARKER}/usr/bin{MARKER}{MARKER}"));
        assert!(env.kubeconfig.is_empty());
        assert_eq!(parse("nothing"), ShellEnv::default());
    }

    #[test]
    fn reads_the_real_login_shell() {
        let env = login_shell_env();
        assert!(env.path.is_some_and(|p| p.contains("/usr/bin")));
    }
}
