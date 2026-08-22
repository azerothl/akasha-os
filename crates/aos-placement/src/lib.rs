//! # aos-placement — Simulateur du Placement Manager (P0.1)
//!
//! Implémente, en Rust standalone, l'algorithme de placement RAM/GPU/disque
//! de `specs-techniques.md` §3.5 :
//!
//! - [`PlacementManager`] : algorithme de placement initial (§3.5.3),
//!   profils `latency` / `balanced` / `memory-saver` / `cpu-only` (§3.5.6),
//!   repli automatique avec suggestion (§16, F-PLC-09) ;
//! - [`CostModel`] : estimation tok/s / TTFT paramétrique, étalonnable sur
//!   mesures llama.cpp (Gate P0) ;
//! - [`PlacementSim`] : état runtime multi-modèles — éviction fair/priority
//!   (F-PLC-06), échelle de pression (§3.5.5), re-profilage à chaud.
//!
//! Les hypothèses du modèle de coût sont documentées dans
//! `adr/0002-model-placement.md`.

pub mod cost;
pub mod hardware;
pub mod manager;
pub mod model;
pub mod plan;
pub mod sim;

pub use cost::{Bound, CostModel, Estimate};
pub use hardware::{GpuDevice, HardwareProfile};
pub use manager::{Budgets, PlacementError, PlacementManager};
pub use model::{KvCacheType, ModelDesc, PrivacyClass};
pub use plan::{PlacementPlan, PlacementProfile, Priority, Shard, ShardKind, Tier};
pub use sim::{PlacedModel, PlacementSim, PressureReport, ReprofileReport, RunState, SimEvent};

/// Modèles de test partagés entre modules.
#[cfg(test)]
pub(crate) mod testutil {
    use crate::model::{ModelDesc, PrivacyClass};

    pub const GIB: u64 = 1 << 30;

    /// 32B Q6 ≈ 26 GiB, 80 couches — exemple de l'annexe A des specs.
    pub fn model_32b() -> ModelDesc {
        ModelDesc {
            id: "local:llama-q6-32b".into(),
            name: "Llama 32B Q6".into(),
            n_layers: 80,
            n_params: 32e9,
            weights_bytes: 26 * GIB,
            embed_bytes: 800_000_000,
            kv_bytes_per_token: 400_000,
            context_length: 131072,
            supports_layer_offload: true,
            privacy_class: PrivacyClass::Local,
        }
    }

    /// 3B Q4 ≈ 2 GiB, 28 couches — modèle embarqué (§3.4).
    pub fn model_3b() -> ModelDesc {
        ModelDesc {
            id: "local:embedded-instruct".into(),
            name: "Embedded Instruct 3B Q4".into(),
            n_layers: 28,
            n_params: 3e9,
            weights_bytes: 2 * GIB,
            embed_bytes: 200_000_000,
            kv_bytes_per_token: 120_000,
            context_length: 8192,
            supports_layer_offload: true,
            privacy_class: PrivacyClass::Local,
        }
    }

    /// SD 1.5 ≈ 4 GiB — pack image E16 (un shard MediaWeights).
    pub fn model_sd15() -> ModelDesc {
        ModelDesc {
            id: "local:sd-v1-5".into(),
            name: "Stable Diffusion 1.5".into(),
            n_layers: 0,
            n_params: 8.6e8,
            weights_bytes: 4 * GIB,
            embed_bytes: 0,
            kv_bytes_per_token: 0,
            context_length: 0,
            supports_layer_offload: false,
            privacy_class: PrivacyClass::Local,
        }
    }

    /// Voix Piper ~64 MiB — TTS CPU, pas de VRAM.
    pub fn model_piper() -> ModelDesc {
        ModelDesc {
            id: "local:piper-en-us".into(),
            name: "Piper en_US".into(),
            n_layers: 0,
            n_params: 1.0e7,
            weights_bytes: 64 * 1024 * 1024,
            embed_bytes: 0,
            kv_bytes_per_token: 0,
            context_length: 0,
            supports_layer_offload: false,
            privacy_class: PrivacyClass::Local,
        }
    }

    /// 70B Q4 ≈ 40 GiB, 80 couches — dépasse la RAM de la machine de référence.
    pub fn model_70b() -> ModelDesc {
        ModelDesc {
            id: "local:llama-q4-70b".into(),
            name: "Llama 70B Q4".into(),
            n_layers: 80,
            n_params: 70e9,
            weights_bytes: 40 * GIB,
            embed_bytes: 1_200_000_000,
            kv_bytes_per_token: 640_000,
            context_length: 131072,
            supports_layer_offload: true,
            privacy_class: PrivacyClass::Local,
        }
    }
}
