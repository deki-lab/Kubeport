//! États d'un Forward : État voulu (le bouton) et État réel (le tunnel).

/// État réel d'un Forward Actif.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RealState {
    /// Le port local écoute et un Pod résolu reçoit les nouvelles connexions.
    Connected { pod: String },
    /// Cause passagère : l'app réessaie seule.
    Reconnecting(String),
    /// Cause qui demande une action humaine : plus de nouvelle tentative.
    Error(String),
}

/// Vue publique d'un Forward : État voulu et, s'il est Actif, État réel.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ForwardStatus {
    pub active: bool,
    pub real: Option<RealState>,
}

impl std::fmt::Display for RealState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RealState::Connected { pod } => write!(f, "Connecté → {pod}"),
            RealState::Reconnecting(reason) => write!(f, "Reconnexion : {reason}"),
            RealState::Error(reason) => write!(f, "Erreur : {reason}"),
        }
    }
}
