//! Configuration des Forwards, stockée en TOML et éditable à la main.
//!
//! ```toml
//! [[forward]]
//! name = "postgres-staging"
//! context = "docker-desktop"
//! namespace = "db"
//! service = "postgres"     # ou : pod = "postgres-0"
//! remote_port = 5432
//! local_port = 5432        # optionnel, vaut remote_port par défaut
//! ```

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Ressource Kubernetes visée par un Forward.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Service(String),
    Pod(String),
}

/// Un port-forward configuré. Son `name` l'identifie de façon unique.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawForward", into = "RawForward")]
pub struct Forward {
    pub name: String,
    pub context: String,
    pub namespace: String,
    pub target: Target,
    pub remote_port: u16,
    pub local_port: u16,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    /// Réglage : kubeconfig forcé. Absent : celui du shell de login, puis `~/.kube/config`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kubeconfig: Option<PathBuf>,
    #[serde(default, rename = "forward")]
    pub forwards: Vec<Forward>,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("lecture de {path} : {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("config invalide : {0}")]
    Parse(#[from] toml::de::Error),
    #[error("écriture de la config : {0}")]
    Serialize(#[from] toml::ser::Error),
    #[error("deux Forwards portent le nom « {0} »")]
    DuplicateName(String),
    #[error("{0}")]
    Invalid(String),
}

/// En-tête écrit en tête du fichier quand l'app l'enregistre.
const HEADER: &str = "# Forwards Kubeport : modifiables ici ou depuis l'app (Recharger après une édition).\n# Un bloc [[forward]] par port-forward ; `service` ou `pod`, pas les deux.\n\n";

/// Écrit dans un fichier temporaire puis renomme : jamais de fichier à moitié écrit.
pub(crate) fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(tmp, path)
}

impl Config {
    /// Emplacement par défaut : `~/Library/Application Support/k8s_forwarder/config.toml`.
    pub fn default_path() -> PathBuf {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        home.join("Library/Application Support/k8s_forwarder/config.toml")
    }

    pub fn parse(text: &str) -> Result<Self, ConfigError> {
        let config: Config = toml::from_str(text)?;
        let mut seen = HashSet::new();
        for forward in &config.forwards {
            if !seen.insert(forward.name.as_str()) {
                return Err(ConfigError::DuplicateName(forward.name.clone()));
            }
        }
        Ok(config)
    }

    /// Charge la config ; un fichier absent donne une config vide.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(source) => Err(ConfigError::Io {
                path: path.to_owned(),
                source,
            }),
        }
    }

    pub fn to_toml(&self) -> Result<String, ConfigError> {
        Ok(toml::to_string_pretty(self)?)
    }

    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        let text = format!("{HEADER}{}", self.to_toml()?);
        write_atomic(path, &text).map_err(|source| ConfigError::Io {
            path: path.to_owned(),
            source,
        })
    }

    /// Ajoute un Forward, ou remplace celui nommé `previous` (qui peut être renommé).
    pub fn upsert(&mut self, previous: Option<&str>, forward: Forward) -> Result<(), ConfigError> {
        validate(&forward)?;
        let index = previous.and_then(|p| self.forwards.iter().position(|f| f.name == p));
        let clash = self
            .forwards
            .iter()
            .enumerate()
            .any(|(i, f)| f.name == forward.name && Some(i) != index);
        if clash {
            return Err(ConfigError::DuplicateName(forward.name));
        }
        match index {
            Some(i) => self.forwards[i] = forward,
            None => self.forwards.push(forward),
        }
        Ok(())
    }

    pub fn remove(&mut self, name: &str) {
        self.forwards.retain(|f| f.name != name);
    }

    pub fn get(&self, name: &str) -> Option<&Forward> {
        self.forwards.iter().find(|f| f.name == name)
    }

    /// Forwards concurrents de `name` : ceux qui partagent son port local.
    pub fn rivals(&self, name: &str) -> Vec<&Forward> {
        let Some(forward) = self.get(name) else {
            return Vec::new();
        };
        self.forwards
            .iter()
            .filter(|f| f.name != name && f.local_port == forward.local_port)
            .collect()
    }
}

fn validate(forward: &Forward) -> Result<(), ConfigError> {
    let target = match &forward.target {
        Target::Service(n) | Target::Pod(n) => n,
    };
    let missing = [
        ("nom", &forward.name),
        ("contexte", &forward.context),
        ("namespace", &forward.namespace),
        ("cible", target),
    ]
    .into_iter()
    .find(|(_, v)| v.trim().is_empty());
    if let Some((field, _)) = missing {
        return Err(ConfigError::Invalid(format!("{field} manquant")));
    }
    if forward.remote_port == 0 || forward.local_port == 0 {
        return Err(ConfigError::Invalid("le port 0 est interdit".into()));
    }
    Ok(())
}

/// Forme TOML d'un Forward : `service` et `pod` sont deux clés exclusives.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawForward {
    name: String,
    context: String,
    namespace: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    service: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pod: Option<String>,
    remote_port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    local_port: Option<u16>,
}

impl TryFrom<RawForward> for Forward {
    type Error = String;

    fn try_from(raw: RawForward) -> Result<Self, Self::Error> {
        let target = match (raw.service, raw.pod) {
            (Some(service), None) => Target::Service(service),
            (None, Some(pod)) => Target::Pod(pod),
            _ => {
                return Err(format!(
                    "Forward « {} » : indiquer soit `service`, soit `pod`",
                    raw.name
                ));
            }
        };
        if raw.remote_port == 0 || raw.local_port == Some(0) {
            return Err(format!("Forward « {} » : le port 0 est interdit", raw.name));
        }
        Ok(Forward {
            local_port: raw.local_port.unwrap_or(raw.remote_port),
            name: raw.name,
            context: raw.context,
            namespace: raw.namespace,
            target,
            remote_port: raw.remote_port,
        })
    }
}

impl From<Forward> for RawForward {
    fn from(f: Forward) -> Self {
        let (service, pod) = match f.target {
            Target::Service(s) => (Some(s), None),
            Target::Pod(p) => (None, Some(p)),
        };
        RawForward {
            local_port: (f.local_port != f.remote_port).then_some(f.local_port),
            name: f.name,
            context: f.context,
            namespace: f.namespace,
            service,
            pod,
            remote_port: f.remote_port,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
[[forward]]
name = "pg-staging"
context = "staging"
namespace = "db"
service = "postgres"
remote_port = 5432

[[forward]]
name = "pg-prod"
context = "prod"
namespace = "db"
service = "postgres"
remote_port = 5432

[[forward]]
name = "redis-0"
context = "prod"
namespace = "cache"
pod = "redis-0"
remote_port = 6379
local_port = 16379
"#;

    #[test]
    fn parse_les_deux_types_de_cible_et_le_port_local_par_defaut() {
        let config = Config::parse(SAMPLE).unwrap();
        let pg = config.get("pg-staging").unwrap();
        assert_eq!(pg.target, Target::Service("postgres".into()));
        assert_eq!(pg.local_port, 5432);
        let redis = config.get("redis-0").unwrap();
        assert_eq!(redis.target, Target::Pod("redis-0".into()));
        assert_eq!(redis.local_port, 16379);
    }

    #[test]
    fn aller_retour_toml_sans_perte() {
        let config = Config::parse(SAMPLE).unwrap();
        let again = Config::parse(&config.to_toml().unwrap()).unwrap();
        assert_eq!(config, again);
    }

    #[test]
    fn refuse_service_et_pod_en_meme_temps() {
        let text = r#"
[[forward]]
name = "x"
context = "c"
namespace = "n"
service = "s"
pod = "p"
remote_port = 80
"#;
        let err = Config::parse(text).unwrap_err().to_string();
        assert!(err.contains("soit `service`, soit `pod`"), "{err}");
    }

    #[test]
    fn refuse_deux_noms_identiques() {
        let text = SAMPLE.replace("name = \"pg-prod\"", "name = \"pg-staging\"");
        assert!(matches!(
            Config::parse(&text),
            Err(ConfigError::DuplicateName(n)) if n == "pg-staging"
        ));
    }

    #[test]
    fn rivaux_partagent_le_port_local() {
        let config = Config::parse(SAMPLE).unwrap();
        let rivals: Vec<_> = config
            .rivals("pg-staging")
            .iter()
            .map(|f| f.name.as_str())
            .collect();
        assert_eq!(rivals, ["pg-prod"]);
        assert!(config.rivals("redis-0").is_empty());
    }

    #[test]
    fn upsert_ajoute_renomme_et_refuse_les_doublons() {
        let mut config = Config::parse(SAMPLE).unwrap();
        let mut pg = config.get("pg-prod").unwrap().clone();
        pg.name = "pg-production".into();
        config.upsert(Some("pg-prod"), pg.clone()).unwrap();
        assert!(config.get("pg-prod").is_none());
        assert_eq!(config.forwards[1].name, "pg-production");

        pg.name = "pg-staging".into();
        assert!(matches!(
            config.upsert(None, pg.clone()),
            Err(ConfigError::DuplicateName(_))
        ));
        pg.name = " ".into();
        assert!(matches!(
            config.upsert(None, pg),
            Err(ConfigError::Invalid(_))
        ));
    }

    #[test]
    fn save_puis_load_avec_en_tete() {
        let dir = std::env::temp_dir().join(format!("kubeport-config-{}", std::process::id()));
        let path = dir.join("config.toml");
        let mut config = Config::parse(SAMPLE).unwrap();
        config.remove("redis-0");
        config.save(&path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("# Forwards Kubeport"));
        assert_eq!(Config::load(&path).unwrap(), config);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn fichier_absent_donne_config_vide() {
        let config = Config::load(Path::new("/nonexistent/kubeport.toml")).unwrap();
        assert!(config.forwards.is_empty());
    }
}
