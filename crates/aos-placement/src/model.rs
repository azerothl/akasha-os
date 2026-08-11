//! Description d'un modèle de poids (entrée du Placement Manager).
//!
//! Aligné sur les métadonnées du Model Registry (specs-techniques §3.2),
//! avec les champs dérivés nécessaires au placement (taille par couche,
//! tables d'embedding, KV cache par token).

use serde::{Deserialize, Serialize};

/// Classe de privacy d'un modèle (routage §3.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyClass {
    Local,
    Remote,
}

/// Description statique d'un modèle à placer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelDesc {
    pub id: String,
    pub name: String,
    /// Nombre de couches transformer.
    pub n_layers: u32,
    /// Nombre de paramètres (pour l'estimation FLOPs du prefill).
    pub n_params: f64,
    /// Taille totale des poids (octets), toutes tables comprises.
    pub weights_bytes: u64,
    /// Tables embedding + output (octets) — placées selon hotness (§3.5.2).
    pub embed_bytes: u64,
    /// KV cache par token de contexte, tous layers confondus (octets).
    pub kv_bytes_per_token: u64,
    pub context_length: u32,
    pub supports_layer_offload: bool,
    pub privacy_class: PrivacyClass,
}

impl ModelDesc {
    /// Poids d'une couche transformer (octets), hors tables embed/output.
    pub fn layer_bytes(&self) -> u64 {
        (self.weights_bytes - self.embed_bytes) / u64::from(self.n_layers)
    }

    /// Paramètres par couche (hors tables, supposées minoritaires).
    pub fn layer_params(&self) -> f64 {
        self.n_params / f64::from(self.n_layers)
    }

    /// Taille du KV cache pour `tokens` de contexte effectif.
    pub fn kv_bytes(&self, tokens: u32) -> u64 {
        self.kv_bytes_per_token * u64::from(tokens)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_bytes_exclut_les_tables() {
        let m = ModelDesc {
            id: "m".into(),
            name: "m".into(),
            n_layers: 10,
            n_params: 1e9,
            weights_bytes: 1_000_000_000,
            embed_bytes: 100_000_000,
            kv_bytes_per_token: 100_000,
            context_length: 4096,
            supports_layer_offload: true,
            privacy_class: PrivacyClass::Local,
        };
        assert_eq!(m.layer_bytes(), 90_000_000);
        assert_eq!(m.kv_bytes(2048), 204_800_000);
    }
}
