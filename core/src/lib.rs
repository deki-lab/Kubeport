//! Cœur de Kubeport : Forwards, résolution du Pod résolu, États voulu et réel.
//!
//! Vocabulaire : voir `GLOSSARY.md`. Décisions : voir `docs/adr/`.

pub mod catalog;
pub mod config;
mod forward;
pub mod manager;
pub mod resolve;
pub mod session;
pub mod shell_env;
pub mod state;

pub use config::{Config, ConfigError, Field, Forward, Target};
pub use manager::{KubeEnv, Manager, ManagerError, Statuses};
pub use shell_env::{ShellEnv, login_shell_env};
pub use state::{ForwardStatus, RealState, Reason};
