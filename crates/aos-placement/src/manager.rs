//! Placement Manager — algorithme de placement initial (§3.5.3).
//!
//! Le `PlacementManager` est **sans état** : il calcule un plan à partir d'un
//! modèle, d'un profil et de budgets libres. L'état partagé (plusieurs
//! modèles placés, éviction, pression) est géré par [`crate::sim::PlacementSim`].

use crate::cost::{CostModel, Estimate};
use crate::hardware::HardwareProfile;
use crate::model::ModelDesc;
use crate::plan::{PlacementPlan, PlacementProfile, Priority, Shard, ShardKind, Tier};
use thiserror::Error;

/// Budgets libres par tier (après réserves OS et allocations existantes).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Budgets {
    pub vram: u64,
    pub ram: u64,
    pub disk: u64,
}

impl Budgets {
    pub fn full(hw: &HardwareProfile) -> Self {
        Self {
            vram: hw.vram_budget(),
            ram: hw.ram_budget(),
            disk: hw.disk_budget(),
        }
    }
}

/// Erreurs de placement (contrats §16).
#[derive(Debug, Clone, Error)]
pub enum PlacementError {
    #[error("placement impossible: {reason} ; suggestion: {suggestion}")]
    Impossible { reason: String, suggestion: String },
    #[error("plan sous le seuil de viabilité: {tok_s:.2} tok/s < {min:.2} tok/s")]
    BelowViability { tok_s: f64, min: f64 },
    #[error("le modèle ne supporte pas l'offload de couches et dépasse VRAM+RAM")]
    OffloadUnsupported,
}

/// Contexte KV maximal en mode micro-batch (`memory-saver`, §3.5.6).
pub const MICRO_BATCH_KV_TOKENS: u32 = 512;

/// Fraction du budget couche VRAM consommable en `balanced` (marge pour
/// croissance KV et cohabitation de modèles).
pub const BALANCED_VRAM_FILL: f64 = 0.85;

/// Fraction maximale des octets de couches logeables en RAM en `memory-saver`.
pub const MEMORY_SAVER_RAM_SHARE: f64 = 0.25;

/// Placement Manager (stateless).
#[derive(Debug, Clone)]
pub struct PlacementManager {
    pub hw: HardwareProfile,
    pub cost: CostModel,
}

impl PlacementManager {
    pub fn new(hw: HardwareProfile, cost: CostModel) -> Self {
        Self { hw, cost }
    }

    /// Score de hotness des couches (§3.5.3) : les couches d'entrée/sortie
    /// sont typiquement plus chaudes ; le milieu dépend du modèle.
    ///
    /// Retourne les indices de couches triés par score décroissant.
    pub fn score_layers(model: &ModelDesc) -> Vec<u32> {
        let n = model.n_layers;
        let edge = (n / 8).max(2) as i64;
        let mut scored: Vec<(u32, f64)> = (0..n)
            .map(|i| {
                let d_edge = (i as i64).min((n - 1 - i) as i64);
                let bonus = ((edge - d_edge).max(0) as f64 / edge as f64) * 0.5;
                (i, 1.0 + bonus)
            })
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap().then(a.0.cmp(&b.0)));
        scored.into_iter().map(|(i, _)| i).collect()
    }

    /// Calcule un plan de placement (§3.5.3) dans des budgets donnés.
    pub fn place_model_with_budgets(
        &self,
        model: &ModelDesc,
        profile: PlacementProfile,
        kv_tokens: u32,
        budgets: Budgets,
    ) -> Result<PlacementPlan, PlacementError> {
        if model.is_media() {
            return self.place_media_with_budgets(model, profile, budgets);
        }
        let effective_profile = if !self.hw.has_gpu {
            PlacementProfile::CpuOnly
        } else {
            profile
        };

        let kv_tokens_eff = match effective_profile {
            PlacementProfile::MemorySaver => kv_tokens.min(MICRO_BATCH_KV_TOKENS),
            _ => kv_tokens,
        };
        let kv_bytes = model.kv_bytes(kv_tokens_eff);
        let embed_bytes = model.embed_bytes;
        let layer_bytes = model.layer_bytes();

        let mut free = budgets;
        let mut shards: Vec<Shard> = Vec::with_capacity(model.n_layers as usize + 2);
        let mut next_shard = 0u32;
        let mut push = |kind: ShardKind, size: u64, tier: Tier, shards: &mut Vec<Shard>| {
            shards.push(Shard {
                model_id: model.id.clone(),
                shard_id: next_shard,
                kind,
                size_bytes: size,
                residency: tier,
                device: None,
                pin_count: 0,
                last_use_tick: 0,
                priority_boost: 0,
            });
            next_shard += 1;
        };

        // --- KV cache (§3.5.3 `place_kv_policy`) : VRAM puis RAM ; DISK
        // seulement en mode extrême. ---
        let kv_tier = self.place_unit(&mut free, kv_bytes, effective_profile, true);
        let kv_tier = match kv_tier {
            Some(t) => t,
            None => {
                return Err(PlacementError::Impossible {
                    reason: format!("KV cache ({kv_bytes} o) ne tient nulle part"),
                    suggestion: "réduire le contexte ou passer en memory-saver".into(),
                })
            }
        };
        push(ShardKind::KvCache, kv_bytes, kv_tier, &mut shards);

        // --- Tables embed/output : hotness maximale (lues à chaque token).
        let embed_tier = self
            .place_unit(&mut free, embed_bytes, effective_profile, false)
            .ok_or_else(|| PlacementError::Impossible {
                reason: format!("tables embed ({embed_bytes} o) ne tiennent nulle part"),
                suggestion: "libérer de la mémoire ou réduire le modèle".into(),
            })?;
        push(ShardKind::Embed, embed_bytes, embed_tier, &mut shards);

        // --- Couches : score décroissant, VRAM selon profil, puis RAM, puis DISK.
        let vram_layer_budget = match effective_profile {
            PlacementProfile::Latency => free.vram,
            PlacementProfile::Balanced => (free.vram as f64 * BALANCED_VRAM_FILL) as u64,
            PlacementProfile::MemorySaver | PlacementProfile::CpuOnly => 0,
        };
        let ram_layer_budget = match effective_profile {
            PlacementProfile::MemorySaver => free
                .ram
                .min((model.weights_bytes as f64 * MEMORY_SAVER_RAM_SHARE) as u64),
            _ => free.ram,
        };

        let mut vram_left = vram_layer_budget;
        let mut ram_left = ram_layer_budget;
        let mut disk_left = free.disk;
        let mut n_disk = 0u32;

        for idx in Self::score_layers(model) {
            let tier = if vram_left >= layer_bytes {
                vram_left -= layer_bytes;
                Tier::Vram
            } else if ram_left >= layer_bytes {
                ram_left -= layer_bytes;
                Tier::Ram
            } else if disk_left >= layer_bytes {
                disk_left -= layer_bytes;
                n_disk += 1;
                Tier::Disk
            } else {
                return Err(PlacementError::Impossible {
                    reason: format!(
                        "couche {idx} ({layer_bytes} o) sans tier disponible (modèle trop gros)"
                    ),
                    suggestion: "quantifier davantage ou ajouter de la mémoire".into(),
                });
            };
            push(ShardKind::Layer(idx), layer_bytes, tier, &mut shards);
        }

        if n_disk > 0 && !model.supports_layer_offload {
            return Err(PlacementError::OffloadUnsupported);
        }

        // Tri par index de couche pour un ordre de forward cohérent.
        shards.sort_by_key(|s| s.shard_id);
        let mut plan = PlacementPlan {
            model_id: model.id.clone(),
            profile: effective_profile,
            shards,
            prefetch_window: 2,
            kv_tokens: kv_tokens_eff,
            tensor_split: vec![],
            main_gpu: 0,
        };
        self.apply_multi_gpu_partition(&mut plan);

        // --- validate_plan (§3.5.3) : seuil critique de tok/s. ---
        let est = self.estimate(&plan, model, 256, kv_tokens_eff);
        if !est.viable {
            return Err(PlacementError::BelowViability {
                tok_s: est.tok_s,
                min: self.cost.min_tok_s,
            });
        }
        Ok(plan)
    }

    /// `place_model` simple : budgets complets de la machine (aucun autre
    /// modèle placé).
    pub fn place_model(
        &self,
        model: &ModelDesc,
        profile: PlacementProfile,
        _priority: Priority,
        kv_tokens: u32,
    ) -> Result<PlacementPlan, PlacementError> {
        self.place_model_with_budgets(model, profile, kv_tokens, Budgets::full(&self.hw))
    }

    /// Repli automatique de profil (§16) : essaie le profil demandé puis ses
    /// fallbacks jusqu'à validation ; erreur explicite avec suggestion sinon
    /// (F-PLC-09).
    pub fn place_auto(
        &self,
        model: &ModelDesc,
        profile: PlacementProfile,
        kv_tokens: u32,
        budgets: Budgets,
    ) -> Result<PlacementPlan, PlacementError> {
        let mut current = Some(profile);
        let mut last_err = None;
        while let Some(p) = current {
            match self.place_model_with_budgets(model, p, kv_tokens, budgets) {
                Ok(plan) => return Ok(plan),
                Err(e) => {
                    last_err = Some(e);
                    current = p.fallback();
                }
            }
        }
        Err(PlacementError::Impossible {
            reason: format!("aucun profil ne permet de placer {}", model.id),
            suggestion: match last_err {
                Some(e) => e.to_string(),
                None => "modèle trop grand pour cette machine".into(),
            },
        })
    }

    /// Place un modèle image/TTS comme un unique shard `MediaWeights` (E16).
    /// GPU préféré pour l'image ; RAM/DISK si VRAM insuffisante (TTS CPU).
    fn place_media_with_budgets(
        &self,
        model: &ModelDesc,
        profile: PlacementProfile,
        budgets: Budgets,
    ) -> Result<PlacementPlan, PlacementError> {
        let effective_profile = if !self.hw.has_gpu {
            PlacementProfile::CpuOnly
        } else {
            profile
        };
        let size = model.weights_bytes.max(1);
        let free = budgets;
        let prefer_vram = matches!(
            effective_profile,
            PlacementProfile::Latency | PlacementProfile::Balanced
        ) && self.hw.has_gpu;
        let tier = if prefer_vram && free.vram >= size {
            Tier::Vram
        } else if free.ram >= size {
            Tier::Ram
        } else if free.disk >= size {
            Tier::Disk
        } else {
            return Err(PlacementError::Impossible {
                reason: format!(
                    "poids média {} ({size} o) ne tiennent nulle part",
                    model.id
                ),
                suggestion: "pack plus petit, TTS CPU, ou décharger le LLM".into(),
            });
        };
        let shard = Shard {
            model_id: model.id.clone(),
            shard_id: 0,
            kind: ShardKind::MediaWeights,
            size_bytes: size,
            residency: tier,
            device: if tier == Tier::Vram { Some(0) } else { None },
            pin_count: 0,
            last_use_tick: 0,
            priority_boost: 0,
        };
        let mut plan = PlacementPlan {
            model_id: model.id.clone(),
            profile: effective_profile,
            shards: vec![shard],
            prefetch_window: 0,
            kv_tokens: 0,
            tensor_split: vec![],
            main_gpu: 0,
        };
        self.apply_multi_gpu_partition(&mut plan);
        Ok(plan)
    }

    /// Partition pipeline inter-GPU (§3.5.7) : fractions `tensor_split` +
    /// `device` sur les shards VRAM (couches contiguës par capacité).
    fn apply_multi_gpu_partition(&self, plan: &mut PlacementPlan) {
        let n = self.hw.n_gpus();
        if n <= 1 {
            plan.tensor_split.clear();
            plan.main_gpu = 0;
            for s in &mut plan.shards {
                if s.residency == Tier::Vram {
                    s.device = Some(0);
                }
            }
            return;
        }

        let caps: Vec<f32> = if self.hw.gpus.len() >= n {
            self.hw.gpus[..n]
                .iter()
                .map(|g| g.vram_total.max(1) as f32)
                .collect()
        } else {
            vec![1.0; n]
        };
        let sum: f32 = caps.iter().sum::<f32>().max(1.0);
        plan.tensor_split = caps.iter().map(|c| c / sum).collect();
        plan.main_gpu = 0;

        let mut layer_idxs: Vec<usize> = plan
            .shards
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                matches!(s.kind, ShardKind::Layer(_)) && s.residency == Tier::Vram
            })
            .map(|(i, _)| i)
            .collect();
        layer_idxs.sort_by_key(|&i| match plan.shards[i].kind {
            ShardKind::Layer(idx) => idx,
            _ => 0,
        });

        let n_layers = layer_idxs.len();
        let mut start = 0usize;
        let mut cum = 0.0f32;
        for (gi, frac) in plan.tensor_split.iter().enumerate() {
            cum += frac;
            let end = if gi + 1 == n {
                n_layers
            } else {
                ((cum * n_layers as f32).round() as usize).clamp(start, n_layers)
            };
            for &si in &layer_idxs[start..end] {
                plan.shards[si].device = Some(gi as u32);
            }
            start = end;
        }
        for s in &mut plan.shards {
            if s.residency == Tier::Vram && s.device.is_none() {
                s.device = Some(0);
            }
        }
    }

    /// Estimation de performance d'un plan (raccourci vers le modèle de coût).
    pub fn estimate(
        &self,
        plan: &PlacementPlan,
        model: &ModelDesc,
        prompt_tokens: u32,
        ctx_tokens: u32,
    ) -> Estimate {
        self.cost
            .estimate(plan, model, &self.hw, prompt_tokens, ctx_tokens)
    }

    /// Place une unité (KV ou embed) selon la politique du profil.
    /// Retourne le tier choisi et décrémente le budget correspondant.
    fn place_unit(
        &self,
        free: &mut Budgets,
        bytes: u64,
        profile: PlacementProfile,
        is_kv: bool,
    ) -> Option<Tier> {
        let prefer_vram = match profile {
            PlacementProfile::Latency | PlacementProfile::Balanced => true,
            // memory-saver : KV minimal en VRAM, embed plutôt en RAM.
            PlacementProfile::MemorySaver => is_kv,
            PlacementProfile::CpuOnly => false,
        };
        if prefer_vram && free.vram >= bytes {
            free.vram -= bytes;
            return Some(Tier::Vram);
        }
        if free.ram >= bytes {
            free.ram -= bytes;
            return Some(Tier::Ram);
        }
        // Mode extrême : KV sur disque (§3.5.3, « jamais DISK sauf mode extrême »).
        if free.disk >= bytes {
            free.disk -= bytes;
            return Some(Tier::Disk);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{model_32b, model_3b, model_70b, GIB};

    fn pm() -> PlacementManager {
        PlacementManager::new(HardwareProfile::reference_v1(), CostModel::default())
    }

    #[test]
    fn modele_plus_petit_que_vram_full_gpu() {
        let m = model_3b();
        let plan = pm()
            .place_model(&m, PlacementProfile::Latency, Priority::Interactive, 2048)
            .unwrap();
        assert_eq!(plan.layer_bytes_on(Tier::Disk), 0);
        assert_eq!(plan.layer_bytes_on(Tier::Ram), 0);
        assert!(plan.layer_bytes_on(Tier::Vram) > 0);
        assert!(plan.kv_bytes_on(Tier::Vram) > 0);
    }

    #[test]
    fn modele_32b_hybride_sur_machine_reference() {
        let m = model_32b();
        let plan = pm()
            .place_model(&m, PlacementProfile::Balanced, Priority::AgentNormal, 2048)
            .unwrap();
        // VRAM (7,5 GiB budget) < modèle (26 GiB) < RAM : hybride attendu.
        assert!(plan.layer_bytes_on(Tier::Vram) > 0);
        assert!(plan.layer_bytes_on(Tier::Ram) > 0);
        println!("{}", plan.summary());
    }

    #[test]
    fn memory_saver_met_la_majorite_sur_disque() {
        let m = model_32b();
        let plan = pm()
            .place_model(&m, PlacementProfile::MemorySaver, Priority::Batch, 2048)
            .unwrap();
        let disk = plan.layer_bytes_on(Tier::Disk);
        assert!(disk > m.weights_bytes / 2, "disk={disk}");
        assert_eq!(plan.kv_tokens, MICRO_BATCH_KV_TOKENS);
    }

    #[test]
    fn cpu_only_n_utilise_pas_la_vram() {
        let m = model_3b();
        let plan = pm()
            .place_model(&m, PlacementProfile::CpuOnly, Priority::AgentNormal, 2048)
            .unwrap();
        assert_eq!(plan.bytes_on(Tier::Vram), 0);
    }

    #[test]
    fn modele_trop_gros_refuse_avec_suggestion() {
        let mut m = model_32b();
        m.weights_bytes = 900 * GIB; // dépasse le disque
        m.embed_bytes = GIB;
        let err = pm()
            .place_model(&m, PlacementProfile::Balanced, Priority::AgentNormal, 2048)
            .unwrap_err();
        match err {
            PlacementError::Impossible { suggestion, .. } => {
                assert!(!suggestion.is_empty())
            }
            other => panic!("attendu Impossible, reçu {other:?}"),
        }
    }

    #[test]
    fn offload_non_supporte_refuse() {
        // 70B (40 GiB) > VRAM (7,5) + RAM (28) de la machine de référence :
        // des couches DISK seraient nécessaires → refus si offload non supporté.
        let mut m = model_70b();
        m.supports_layer_offload = false;
        let err = pm()
            .place_model(&m, PlacementProfile::Balanced, Priority::AgentNormal, 2048)
            .unwrap_err();
        assert!(matches!(err, PlacementError::OffloadUnsupported));
    }

    #[test]
    fn place_auto_replie_sur_profil_moins_gourmand() {
        // Machine CPU-only : demander latency doit replier en cpu-only.
        let pm = PlacementManager::new(HardwareProfile::cpu_only_laptop(), CostModel::default());
        let m = model_3b();
        let plan = pm
            .place_auto(&m, PlacementProfile::Latency, 2048, Budgets::full(&pm.hw))
            .unwrap();
        assert_eq!(plan.profile, PlacementProfile::CpuOnly);
    }

    #[test]
    fn score_layers_privilegie_les_bords() {
        let m = model_32b();
        let order = PlacementManager::score_layers(&m);
        // Les 4 premières couches scorées doivent être des couches de bord.
        for &idx in &order[..4] {
            let d = idx.min(m.n_layers - 1 - idx);
            assert!(d < m.n_layers / 8, "couche {idx} trop centrale");
        }
    }

    #[test]
    fn media_image_prend_vram_quand_disponible() {
        let m = crate::testutil::model_sd15();
        let plan = pm()
            .place_model(&m, PlacementProfile::Balanced, Priority::Interactive, 0)
            .unwrap();
        assert!(m.is_media());
        assert_eq!(plan.shards.len(), 1);
        assert_eq!(plan.shards[0].kind, ShardKind::MediaWeights);
        assert_eq!(plan.shards[0].residency, Tier::Vram);
    }

    #[test]
    fn media_tts_tient_en_ram_cpu_only() {
        let pm = PlacementManager::new(HardwareProfile::cpu_only_laptop(), CostModel::default());
        let m = crate::testutil::model_piper();
        let plan = pm
            .place_auto(&m, PlacementProfile::Latency, 0, Budgets::full(&pm.hw))
            .unwrap();
        assert_eq!(plan.shards[0].kind, ShardKind::MediaWeights);
        assert_ne!(plan.shards[0].residency, Tier::Vram);
    }

    #[test]
    fn multi_gpu_partitionne_tensor_split_et_devices() {
        let pm = PlacementManager::new(HardwareProfile::dual_gpu_8g(), CostModel::default());
        assert_eq!(pm.hw.n_gpus(), 2);
        let m = model_3b();
        let plan = pm
            .place_model(&m, PlacementProfile::Latency, Priority::Interactive, 2048)
            .unwrap();
        assert_eq!(plan.tensor_split.len(), 2);
        let sum: f32 = plan.tensor_split.iter().sum();
        assert!((sum - 1.0).abs() < 1e-3, "split={:?}", plan.tensor_split);
        let vram_layers: Vec<_> = plan
            .shards
            .iter()
            .filter(|s| matches!(s.kind, ShardKind::Layer(_)) && s.residency == Tier::Vram)
            .collect();
        assert!(!vram_layers.is_empty());
        let devices: std::collections::BTreeSet<_> =
            vram_layers.iter().filter_map(|s| s.device).collect();
        assert!(
            devices.len() >= 2,
            "attendu ≥2 GPU sur couches VRAM, devices={devices:?}"
        );
    }

    #[test]
    fn single_gpu_laisse_tensor_split_vide() {
        let plan = pm()
            .place_model(
                &model_3b(),
                PlacementProfile::Latency,
                Priority::Interactive,
                2048,
            )
            .unwrap();
        assert!(plan.tensor_split.is_empty());
        assert_eq!(plan.main_gpu, 0);
    }
}
