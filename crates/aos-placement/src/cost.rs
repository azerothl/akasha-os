//! Modèle de coût : estimation tok/s et TTFT d'un plan de placement.
//!
//! Modèle **paramétrique à étalonner** (Gate P0 : écart < 30 % vs mesures
//! llama.cpp réelles). Les hypothèses et la méthode d'étalonnage sont
//! documentées dans `adr/0002-model-placement.md`.
//!
//! ## Résumé du modèle
//!
//! **Decode** (1 token) — régime limité par la bande passante mémoire :
//! - `t_compute` = octets lus sur les tiers rapides (VRAM à `gpu_bw·eff_gpu`,
//!   RAM à `ram_bw·eff_ram` — les couches RAM sont calculées côté CPU comme
//!   le fait llama.cpp avec `-ngl`), KV cache et tables inclus ;
//! - `t_stream` = octets des couches DISQUE lus à `disk_bw·eff_disk` ;
//! - avec prefetch + double buffering (§3.5.4) : `t_token = max(t_compute,
//!   t_stream) + overhead`.
//!
//! **Prefill** (N tokens) — régime limité par le calcul, tokens batchés :
//! - par tier rapide : `n_layers × max(2·P_layer·N / flops_eff, B_layer / bw_eff)` ;
//! - les couches disque sont streamées une fois, en chevauchement du calcul
//!   des autres tiers : `t_prefill = max(t_rapides, t_stream)`.
//!
//! TTFT = `t_prefill + t_token + t_setup`.

use crate::hardware::HardwareProfile;
use crate::model::ModelDesc;
use crate::plan::{PlacementPlan, Tier};
use serde::{Deserialize, Serialize};

/// Facteurs d'efficience du modèle de coût (calibration).
///
/// Valeurs par défaut étalonnées sur mesures llama.cpp réelles (b10361) —
/// voir `adr/0002-model-placement.md` §Validation croisée :
/// - `eff_ram` : 0,99 mesuré sur hôte Zen4/DDR5 (decode quasi purement
///   limité par la bande passante) ; 0,80 retenu pour la machine de
///   référence (marge pour CPU plus anciens / quants mixtes) ;
/// - `eff_gpu` : en attente de mesure sur machine GPU (P1 au plus tard).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostModel {
    /// Fraction de la bande passante VRAM utile au decode (~0,45–0,6).
    pub eff_gpu: f64,
    /// Fraction de la bande passante RAM utile au decode CPU (mesuré 0,99
    /// sur Zen4+DDR5 ; défaut prudent 0,80).
    pub eff_ram: f64,
    /// Fraction de la bande passante disque utile au streaming.
    /// Étalon ADR 0005 (D2) : DeepNVMe mesure ~66 % du pic NVMe en lecture
    /// réelle tunée (3,69/5,6 GB/s sur A6000 workstation) → 0,65 par défaut,
    /// à recalibrer par hôte en P1 (sonde disque + mmap vs lecture explicite).
    pub eff_disk: f64,
    /// Efficience FLOPs GPU en prefill (~0,5–0,7).
    pub eff_prefill_gpu: f64,
    /// Efficience FLOPs CPU en prefill (étalonnée : flops_eff ≈ 1,0–1,7
    /// TFLOPS sur Zen4 8c pour Q4_K 0,5B–3B).
    pub eff_prefill_cpu: f64,
    /// Surcharge fixe par token décodé (sampling, scheduling) en ms.
    pub overhead_ms: f64,
    /// Surcharge fixe par requête (init graphe, 1er appel) en ms.
    pub setup_ms: f64,
    /// Seuil critique de viabilité (§3.5.3 `validate_plan`), tok/s.
    pub min_tok_s: f64,
}

impl Default for CostModel {
    fn default() -> Self {
        Self {
            eff_gpu: 0.50,
            eff_ram: 0.80,
            eff_disk: 0.65,
            eff_prefill_gpu: 0.60,
            eff_prefill_cpu: 0.50,
            overhead_ms: 0.2,
            setup_ms: 10.0,
            min_tok_s: 0.1,
        }
    }
}

/// Ressource limitante d'un plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Bound {
    /// Limité par le calcul / la bande passante mémoire vive.
    Compute,
    /// Limité par le streaming disque.
    Stream,
}

/// Estimation de performance d'un plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Estimate {
    pub tok_s: f64,
    pub ttft_ms: f64,
    /// Décomposition du temps par token (ms).
    pub t_compute_ms: f64,
    pub t_stream_ms: f64,
    pub decode_bound: Bound,
    /// Contexte KV supposé dans l'estimation.
    pub kv_tokens: u32,
    /// Vrai si l'estimation passe le seuil critique `min_tok_s`.
    pub viable: bool,
}

impl CostModel {
    /// Estime tok/s et TTFT d'un plan déjà calculé.
    ///
    /// * `prompt_tokens` : taille du prompt (prefill) pour le TTFT ;
    /// * `ctx_tokens` : contexte KV actif supposé pendant le decode.
    pub fn estimate(
        &self,
        plan: &PlacementPlan,
        model: &ModelDesc,
        hw: &HardwareProfile,
        prompt_tokens: u32,
        ctx_tokens: u32,
    ) -> Estimate {
        let layer_v = plan.layer_bytes_on(Tier::Vram) as f64;
        let layer_r = plan.layer_bytes_on(Tier::Ram) as f64;
        let layer_d = plan.layer_bytes_on(Tier::Disk) as f64;

        // Embed + KV : lus à chaque token sur leur tier de résidence.
        let kv_tokens = ctx_tokens.min(plan.kv_tokens);
        let kv_total = model.kv_bytes(kv_tokens) as f64;
        let kv_v = plan.kv_bytes_on(Tier::Vram) as f64 / plan.kv_bytes_on_all().max(1.0);
        let kv_r = plan.kv_bytes_on(Tier::Ram) as f64 / plan.kv_bytes_on_all().max(1.0);
        let kv_d = plan.kv_bytes_on(Tier::Disk) as f64 / plan.kv_bytes_on_all().max(1.0);

        let gpu_bw = hw.gpu_mem_bw * self.eff_gpu;
        let ram_bw = hw.ram_mem_bw * self.eff_ram;
        let disk_bw = hw.disk_seq_bw * self.eff_disk;

        let read_v = layer_v + plan.embed_bytes_on(Tier::Vram) as f64 + kv_total * kv_v;
        let read_r = layer_r + plan.embed_bytes_on(Tier::Ram) as f64 + kv_total * kv_r;
        let read_d = layer_d + plan.embed_bytes_on(Tier::Disk) as f64 + kv_total * kv_d;

        let t_compute = if hw.has_gpu && gpu_bw > 0.0 {
            read_v / gpu_bw
        } else {
            // Pas de GPU : tout retombe sur la RAM (cpu-only).
            (read_v + read_r) / ram_bw
        } + if hw.has_gpu { read_r / ram_bw } else { 0.0 };

        let t_stream = read_d / disk_bw;

        // Prefetch + double buffering (§3.5.4) : le streaming disque se
        // chevauche avec le calcul des tiers rapides.
        let t_token = t_compute.max(t_stream) + self.overhead_ms / 1000.0;
        let tok_s = 1.0 / t_token;

        // --- Prefill ---
        let n = f64::from(prompt_tokens);
        let p_layer = model.layer_params();
        let b_layer = model.layer_bytes() as f64;
        let n_v = plan.n_layers_on(Tier::Vram) as f64;
        let n_r = plan.n_layers_on(Tier::Ram) as f64;

        let flops_gpu = hw.gpu_flops * self.eff_prefill_gpu;
        let flops_cpu = hw.cpu_flops * self.eff_prefill_cpu;

        let t_pf_vram = if hw.has_gpu && flops_gpu > 0.0 {
            n_v * (2.0 * p_layer * n / flops_gpu).max(b_layer / gpu_bw)
        } else {
            0.0
        };
        // Couches RAM : calcul CPU. En cpu-only, les couches "VRAM" n'existent
        // pas (le plan les a déjà mises en RAM).
        let t_pf_ram = n_r * (2.0 * p_layer * n / flops_cpu).max(b_layer / ram_bw);
        let t_pf_stream = read_d / disk_bw;

        let t_prefill = (t_pf_vram + t_pf_ram).max(t_pf_stream);
        let ttft_ms = (t_prefill + t_token) * 1000.0 + self.setup_ms;

        Estimate {
            tok_s,
            ttft_ms,
            t_compute_ms: t_compute * 1000.0,
            t_stream_ms: t_stream * 1000.0,
            decode_bound: if t_stream > t_compute {
                Bound::Stream
            } else {
                Bound::Compute
            },
            kv_tokens,
            viable: tok_s >= self.min_tok_s,
        }
    }

    /// Étalonner un facteur d'efficience à partir d'une mesure réelle.
    ///
    /// * `measured_tok_s` : tok/s mesuré sur llama.cpp pour un placement
    ///   dont les octets sont **intégralement** sur le tier visé ;
    /// * retourne le facteur d'efficience qui reproduit la mesure.
    ///
    /// Méthode : le decode étant `1 / (bytes / (bw·eff) + overhead)`, on
    /// résout `eff` analytiquement.
    pub fn solve_efficiency(&self, bytes: f64, raw_bw: f64, measured_tok_s: f64) -> f64 {
        let t = 1.0 / measured_tok_s - self.overhead_ms / 1000.0;
        if t <= 0.0 {
            return 1.0;
        }
        (bytes / (raw_bw * t)).clamp(0.01, 1.0)
    }
}

impl PlacementPlan {
    /// Octets KV totaux du plan (tous tiers).
    pub fn kv_bytes_on_all(&self) -> f64 {
        self.kv_bytes_on(Tier::Vram) as f64
            + self.kv_bytes_on(Tier::Ram) as f64
            + self.kv_bytes_on(Tier::Disk) as f64
    }

    /// Nombre de couches sur un tier.
    pub fn n_layers_on(&self, tier: Tier) -> usize {
        self.shards
            .iter()
            .filter(|s| s.residency == tier && matches!(s.kind, crate::plan::ShardKind::Layer(_)))
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{PlacementProfile, Priority, ShardKind};
    use crate::PlacementManager;

    fn model_3b() -> ModelDesc {
        const GIB: u64 = 1 << 30;
        ModelDesc {
            id: "m3b".into(),
            name: "3B Q4".into(),
            n_layers: 28,
            n_params: 3e9,
            weights_bytes: 2 * GIB,
            embed_bytes: 200_000_000,
            kv_bytes_per_token: 120_000,
            context_length: 8192,
            supports_layer_offload: true,
            privacy_class: crate::model::PrivacyClass::Local,
        }
    }

    #[test]
    fn full_gpu_plus_rapide_que_full_ram() {
        let hw = HardwareProfile::reference_v1();
        let cost = CostModel::default();
        let m = model_3b();
        let pm = PlacementManager::new(hw.clone(), cost.clone());

        let plan_gpu = pm
            .place_model(&m, PlacementProfile::Latency, Priority::Interactive, 2048)
            .unwrap();
        let plan_cpu = pm
            .place_model(&m, PlacementProfile::CpuOnly, Priority::Interactive, 2048)
            .unwrap();

        let e_gpu = cost.estimate(&plan_gpu, &m, &hw, 256, 2048);
        let e_cpu = cost.estimate(&plan_cpu, &m, &hw, 256, 2048);
        assert!(
            e_gpu.tok_s > 2.0 * e_cpu.tok_s,
            "GPU {} vs CPU {}",
            e_gpu.tok_s,
            e_cpu.tok_s
        );
        assert!(e_gpu.ttft_ms < e_cpu.ttft_ms);
    }

    #[test]
    fn calibration_resout_un_facteur_plausible() {
        let cost = CostModel::default();
        // 2 GiB lus à 300 GB/s brut, mesuré 70 tok/s → eff ≈ 0,47.
        let eff = cost.solve_efficiency(2.0 * (1u64 << 30) as f64, 300e9, 70.0);
        assert!((0.4..=0.6).contains(&eff), "eff={eff}");
    }

    #[test]
    fn shard_kind_used() {
        let _ = ShardKind::Embed;
        let _ = ShardKind::MediaWeights;
    }
}
