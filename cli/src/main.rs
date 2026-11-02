//! CLI de développement : exerce `kubeport-core` sans l'interface.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use kubeport_core::{Config, Manager, Target};

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
        Command::Up { names } => up(config, cli.kubeconfig, names).await,
    }
}

async fn up(config: Config, kubeconfig: Option<PathBuf>, names: Vec<String>) -> ExitCode {
    let names = if names.is_empty() {
        config.forwards.iter().map(|f| f.name.clone()).collect()
    } else {
        names
    };
    let env = kubeport_core::KubeEnv {
        kubeconfig: kubeconfig.into_iter().collect(),
        exec_path: None,
    };
    let mut manager = Manager::new(config, env);
    let mut statuses = manager.subscribe();

    for name in &names {
        match manager.activate(name).await {
            Ok(rivals) => {
                for rival in rivals {
                    println!("{rival} deactivated (local port shared with {name})");
                }
            }
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::FAILURE;
            }
        }
    }

    let mut last = statuses.borrow_and_update().clone();
    print_changes(&Default::default(), &last);
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => break,
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
    manager.deactivate_all();
    ExitCode::SUCCESS
}

fn print_changes(before: &kubeport_core::Statuses, after: &kubeport_core::Statuses) {
    for (name, status) in after {
        if before.get(name) != Some(status)
            && let Some(real) = &status.real
        {
            println!("{name:<24} {real}");
        }
    }
}
