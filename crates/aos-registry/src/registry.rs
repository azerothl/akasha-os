//! Catalogue de modèles YAML (specs-techniques §3.2).

use aos_placement::{ModelDesc, PrivacyClass};
use serde::Deserialize;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("lecture catalogue: {0}")]
    Io(#[from] std::io::Error),
    #[error("parsing YAML: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("modèle introuvable: {0}")]
    NotFound(String),
}

/// Racine du catalogue (`data/models/catalog.yaml`).
#[derive(Debug, Clone, Deserialize)]
pub struct Catalog {
    pub models: Vec<CatalogEntry>,
}

/// Entrée de catalogue — reflète le schéma §3.2, étendu des hints nécessaires
/// au simulateur de placement (`embed_bytes`, `kv_bytes_per_token`).
#[derive(Debug, Clone, Deserialize)]
pub struct CatalogEntry {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub modality: Option<String>,
    #[serde(default)]
    pub format: Option<String>,
    pub source: Source,
    #[serde(default)]
    pub architecture: Option<Architecture>,
    #[serde(default)]
    pub resource_hints: Option<ResourceHints>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub backends_compatible: Vec<String>,
    pub privacy_class: PrivacyClass,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Source {
    LocalFile {
        path: String,
        sha256: Option<String>,
    },
    RemoteApi {
        endpoint: String,
        protocol: String,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct Architecture {
    pub n_layers: u32,
    pub n_params: f64,
    pub context_length: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResourceHints {
    pub weights_bytes: u64,
    #[serde(default)]
    pub embed_bytes: u64,
    #[serde(default)]
    pub kv_bytes_per_token: u64,
    #[serde(default)]
    pub supports_layer_offload: bool,
}

impl CatalogEntry {
    pub fn is_remote(&self) -> bool {
        matches!(self.source, Source::RemoteApi { .. })
    }

    /// Conversion vers la description de placement (modèles locaux seulement).
    pub fn to_model_desc(&self) -> Option<ModelDesc> {
        let arch = self.architecture.as_ref()?;
        let hints = self.resource_hints.as_ref()?;
        if self.is_remote() {
            return None;
        }
        Some(ModelDesc {
            id: self.id.clone(),
            name: self.name.clone(),
            n_layers: arch.n_layers,
            n_params: arch.n_params,
            weights_bytes: hints.weights_bytes,
            embed_bytes: hints.embed_bytes,
            kv_bytes_per_token: hints.kv_bytes_per_token,
            context_length: arch.context_length,
            supports_layer_offload: hints.supports_layer_offload,
            privacy_class: self.privacy_class,
        })
    }
}

/// Registre local de modèles (v1 : fichier YAML signé, §7.1 analogue).
#[derive(Debug, Clone)]
pub struct ModelRegistry {
    catalog: Catalog,
}

impl ModelRegistry {
    /// Parse un catalogue depuis une chaîne YAML.
    pub fn from_yaml(yaml: &str) -> Result<Self, RegistryError> {
        Ok(Self {
            catalog: serde_yaml::from_str(yaml)?,
        })
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, RegistryError> {
        Self::from_yaml(&std::fs::read_to_string(path)?)
    }

    pub fn get(&self, id: &str) -> Result<&CatalogEntry, RegistryError> {
        self.catalog
            .models
            .iter()
            .find(|m| m.id == id)
            .ok_or_else(|| RegistryError::NotFound(id.into()))
    }

    /// Descriptions de placement des modèles locaux.
    pub fn local_models(&self) -> impl Iterator<Item = ModelDesc> + '_ {
        self.catalog.models.iter().filter_map(|m| m.to_model_desc())
    }

    /// Entrées distantes (backends remote, §3.3).
    pub fn remote_models(&self) -> impl Iterator<Item = &CatalogEntry> {
        self.catalog.models.iter().filter(|m| m.is_remote())
    }

    pub fn len(&self) -> usize {
        self.catalog.models.len()
    }

    /// Itère sur toutes les entrées du catalogue.
    pub fn entries(&self) -> impl Iterator<Item = &CatalogEntry> {
        self.catalog.models.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.catalog.models.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
models:
  - id: local:tiny
    name: Tiny 1B
    source:
      type: local_file
      path: /models/tiny.gguf
    architecture:
      n_layers: 16
      n_params: 1.0e9
      context_length: 4096
    resource_hints:
      weights_bytes: 700000000
      embed_bytes: 100000000
      kv_bytes_per_token: 60000
      supports_layer_offload: true
    privacy_class: local
  - id: remote:openai:gpt-x
    name: GPT X
    source:
      type: remote_api
      endpoint: https://api.example.com/v1
      protocol: openai_compatible
    privacy_class: remote
"#;

    #[test]
    fn charge_et_convertit() {
        let reg = ModelRegistry::from_yaml(SAMPLE).unwrap();
        assert_eq!(reg.len(), 2);
        let locals: Vec<_> = reg.local_models().collect();
        assert_eq!(locals.len(), 1);
        assert_eq!(locals[0].n_layers, 16);
        assert_eq!(locals[0].layer_bytes(), 37_500_000);
        let remotes: Vec<_> = reg.remote_models().collect();
        assert_eq!(remotes.len(), 1);
        assert_eq!(remotes[0].privacy_class, PrivacyClass::Remote);
    }

    #[test]
    fn modele_absent_erreur() {
        let reg = ModelRegistry::from_yaml(SAMPLE).unwrap();
        assert!(matches!(reg.get("nope"), Err(RegistryError::NotFound(_))));
    }
}
