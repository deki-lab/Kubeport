//! Kubeport core: Forwards, Resolved pod resolution, Desired and Actual states.
//!
//! Vocabulary: see `GLOSSARY.md`. Decisions: see `docs/adr/`.

pub mod catalog;
pub mod config;
mod forward;
pub mod manager;
pub mod resolve;
pub mod session;
pub mod shell_env;
pub mod state;

pub use config::{Config, ConfigError, Field, Forward, IdleTimeout, Target};
pub use manager::{KubeEnv, Manager, ManagerError, Statuses};
pub use session::{Notice, Session};
pub use shell_env::{ShellEnv, login_shell_env};
pub use state::{ForwardStatus, RealState, Reason};
