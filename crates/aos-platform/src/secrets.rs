//! Secrets (§9.2) : stockage local des clés, distribution restreinte aux
//! **services** (jamais aux agents — les agents obtiennent des caps d'usage).
//!
//! v1 : fichier YAML `var/secrets/keys.yaml` (accès hôte dev). P4 : clé
//! hardware/enveloppe dérivée.

use std::collections::HashMap;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SecretError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("yaml: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("secret inconnu: {0}")]
    NotFound(String),
    #[error("acteur non autorisé à lire un secret brut: {0}")]
    Forbidden(String),
}

/// Le magasin de secrets.
pub struct SecretStore {
    keys: HashMap<String, String>,
}

impl SecretStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, SecretError> {
        let path = path.as_ref();
        if !path.exists() {
            std::fs::create_dir_all(path.parent().unwrap_or(Path::new(".")))?;
            std::fs::write(path, "keys: {}\n")?;
        }
        #[derive(serde::Deserialize)]
        struct File {
            #[serde(default)]
            keys: HashMap<String, String>,
        }
        let file: File = serde_yaml::from_str(&std::fs::read_to_string(path)?)?;
        Ok(Self { keys: file.keys })
    }

    /// Lecture d'un secret brut — réservée aux services système
    /// (`service:*`). Un agent demandant un secret brut est refusé (§9.2).
    pub fn get(&self, name: &str, actor: &str) -> Result<String, SecretError> {
        if !actor.starts_with("service:") {
            return Err(SecretError::Forbidden(actor.into()));
        }
        self.keys
            .get(name)
            .cloned()
            .ok_or_else(|| SecretError::NotFound(name.into()))
    }

    pub fn set(&mut self, name: &str, value: &str, path: &Path) -> Result<(), SecretError> {
        self.keys.insert(name.into(), value.into());
        #[derive(serde::Serialize)]
        struct File<'a> {
            keys: &'a HashMap<String, String>,
        }
        std::fs::write(path, serde_yaml::to_string(&File { keys: &self.keys })?)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn services_seulement() {
        let dir = std::env::temp_dir().join(format!("aos-secrets-{}", std::process::id()));
        let path = dir.join("keys.yaml");
        let mut s = SecretStore::open(&path).unwrap();
        s.set("openai_key", "sk-test", &path).unwrap();
        assert!(s.get("openai_key", "service:modeld").is_ok());
        assert!(matches!(
            s.get("openai_key", "agent:1"),
            Err(SecretError::Forbidden(_))
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
