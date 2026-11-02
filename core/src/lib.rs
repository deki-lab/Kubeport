//! Cœur de Kubeport : Forwards, résolution du Pod résolu, États voulu et réel.
//!
//! Vocabulaire : voir `GLOSSARY.md`. Décisions : voir `docs/adr/`.

pub mod config;
mod forward;
pub mod manager;
pub mod resolve;
pub mod state;

pub use config::{Config, Forward, Target};
pub use manager::{Manager, ManagerError, Statuses};
pub use state::{ForwardStatus, RealState};
