//! Development CLI: exercises `kubeport-core` without the UI.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use kubeport_core::{Config, Notice, Session, Target};

#[derive(Parser)]
#[command(
    name = "kubeport",
    version,
    about = "Kubernetes port-forwards (development CLI)"
)]
struct Cli {
    /// Config file (default: ~/Library/Application Support/k8s_forwarder/config.toml)
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    /// Kubeconfig to use (default: KUBECONFIG, then ~/.kube/config)
    #[arg(long, global = true)]
    kubeconfig: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List configured Forwards
    List,
    /// Activate Forwards (all when no name is given) and print their state until Ctrl-C
    Up { names: Vec<String> },
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "kubeport_core=info".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let path = cli.config.unwrap_or_else(Config::default_path);
    let config = match Config::load(&path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}: {e}", path.display());
            return ExitCode::FAILURE;
        }
    };

    match cli.command {
        Command::List => {
            for f in &config.forwards {
                let target = match &f.target {
                    Target::Service(s) => format!("svc/{s}"),
                    Target::Pod(p) => format!("pod/{p}"),
                };
                println!(
                    "{:<24} localhost:{:<6} → {}/{}/{}:{}",
                    f.name, f.local_port, f.context, f.namespace, target, f.remote_port
                );
            }
            ExitCode::SUCCESS
        }
        Command::Up { names } => up(config, path, cli.kubeconfig, names).await,
    }
}

async fn up(
    config: Config,
    path: PathBuf,
    kubeconfig: Option<PathBuf>,
    names: Vec<String>,
) -> ExitCode {
    let names = if names.is_empty() {
        config.forwards.iter().map(|f| f.name.clone()).collect()
    } else {
        names
    };
    let env = kubeport_core::KubeEnv {
        kubeconfig: kubeconfig.into_iter().collect(),
        exec_path: None,
    };
    let mut session = Session::new(config, path, env).ephemeral();
    let mut statuses = session.statuses();

    for name in &names {
        for notice in session.activate(name).await {
            if let Notice::UnknownForward { name } = &notice {
                eprintln!("no Forward named {name:?}");
                return ExitCode::FAILURE;
            }
            print_notice(&notice);
        }
    }

    let mut last = statuses.borrow_and_update().clone();
    print_changes(&Default::default(), &last);
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => break,
            Some(notice) = session.next_expiry() => print_notice(&notice),
            changed = statuses.changed() => {
                if changed.is_err() {
                    break;
                }
                let now = statuses.borrow_and_update().clone();
                print_changes(&last, &now);
                last = now;
            }
        }
    }
    session.set_all(false).await;
    ExitCode::SUCCESS
}

fn print_notice(notice: &Notice) {
    match notice {
        Notice::RivalDeactivated { rival, by } => {
            println!("{rival} deactivated (local port shared with {by})")
        }
        Notice::IdleDeactivated { name, minutes } => {
            println!("{name:<24} deactivated: no connection for {minutes} min")
        }
        Notice::UnknownForward { name } => println!("no Forward named {name:?}"),
        Notice::ConfigNotReloaded { detail } => {
            println!("config not reloaded: {}", printable(detail))
        }
    }
}

fn print_changes(before: &kubeport_core::Statuses, after: &kubeport_core::Statuses) {
    for (name, status) in after {
        if before.get(name) != Some(status)
            && let Some(real) = &status.real
        {
            println!("{name:<24} {}", printable(&real.to_string()));
        }
    }
}

/// Cluster text with control characters escaped: an API message cannot move the
/// cursor or rewrite earlier lines of the terminal.
fn printable(text: &str) -> String {
    text.chars()
        .flat_map(|c| {
            if c.is_control() {
                c.escape_default().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn printable_escapes_control_characters() {
        assert_eq!(
            super::printable("pod \u{1b}[2Kgone\r\nok"),
            "pod \\u{1b}[2Kgone\\r\\nok"
        );
    }
}
