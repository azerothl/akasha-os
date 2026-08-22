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

/// Type KV utilisé for byte estimates (aligné aos-llama / E20).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KvCacheType {
    F16,
    #[default]
    Q8_0,
}

impl KvCacheType {
    /// Facteur octets vs métadonnées catalogue (supposées F16).
    pub fn bytes_factor(self) -> f64 {
        match self {
            KvCacheType::F16 => 1.0,
            KvCacheType::Q8_0 => 0.5,
        }
    }
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
    /// KV cache par token de contexte, tous layers confondus (octets, base F16).
    pub kv_bytes_per_token: u64,
    pub context_length: u32,
    pub supports_layer_offload: bool,
    pub privacy_class: PrivacyClass,
}

impl ModelDesc {
    /// Image / TTS (E16) : pas de couches transformer, un shard `MediaWeights`.
    pub fn is_media(&self) -> bool {
        self.n_layers == 0 && self.kv_bytes_per_token == 0 && self.weights_bytes > 0
    }

    /// Poids d'une couche transformer (octets), hors tables embed/output.
    pub fn layer_bytes(&self) -> u64 {
        if self.n_layers == 0 {
            return self.weights_bytes.saturating_sub(self.embed_bytes);
        }
        (self.weights_bytes - self.embed_bytes) / u64::from(self.n_layers)
    }

    /// Paramètres par couche (hors tables, supposées minoritaires).
    pub fn layer_params(&self) -> f64 {
        self.n_params / f64::from(self.n_layers)
    }

    /// Taille du KV cache pour `tokens` de contexte effectif (F16 catalogue).
    pub fn kv_bytes(&self, tokens: u32) -> u64 {
        self.kv_bytes_typed(tokens, KvCacheType::F16)
    }

    /// Taille KV avec type de cache (E20 Q8 ≈ 0.5×).
    pub fn kv_bytes_typed(&self, tokens: u32, kv_type: KvCacheType) -> u64 {
        let base = self.kv_bytes_per_token * u64::from(tokens);
        (base as f64 * kv_type.bytes_factor()) as u64
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
        assert_eq!(m.kv_bytes_typed(2048, KvCacheType::Q8_0), 102_400_000);
    }
}
