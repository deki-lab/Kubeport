//! Kubeport core: Forwards, Resolved pod resolution, Desired and Actual states.
//!
//! Vocabulary: see `GLOSSARY.md`. Decisions: see `docs/adr/`.

pub mod catalog;
pub mod cluster;
pub mod config;
mod forward;
pub mod manager;
pub mod resolve;
pub mod session;
pub mod shell_env;
pub mod state;

/// Tests name ports 1, 2, 3…; each test thread gets its own free port per name, so tests
/// running in parallel never share a local port, while equal names still compete.
#[cfg(test)]
pub(crate) fn test_port(name: u16) -> u16 {
    use std::cell::RefCell;
    use std::collections::HashMap;
    thread_local!(static PORTS: RefCell<HashMap<u16, u16>> = RefCell::default());
    PORTS.with(|ports| {
        *ports.borrow_mut().entry(name).or_insert_with(|| {
            std::net::TcpListener::bind("127.0.0.1:0")
                .unwrap()
                .local_addr()
                .unwrap()
                .port()
        })
    })
}

pub use cluster::kube::KubeEnv;
pub use config::{Config, ConfigError, Field, Forward, IdleTimeout, Target};
pub use manager::{Manager, ManagerError, Statuses};
pub use session::{Notice, Session};
pub use shell_env::{ShellEnv, login_shell_env};
pub use state::{ForwardStatus, RealState, Reason};
