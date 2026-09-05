//! Validation croisée du modèle de coût contre mesures llama.cpp réelles
//! (Gate P0 : « erreur d'estimation de tok/s < 30 % »).
//!
//! ## Provenance des mesures
//!
//! - Hôte : AMD Ryzen 7 9800X3D (8c/16t), 64 GiB DDR5, Windows 11 ;
//!   bande passante RAM mesurée par `host_probe` : 45,2 GB/s (4 threads),
//!   42,3 GB/s (8 threads), 30,7 GB/s (1 thread) ;
//! - llama.cpp b10361 (`llama-bench -t 8 -p 128 -n 64`, backend CPU) ;
//! - Modèles : Qwen2.5-0.5B-Instruct Q4_K_M et Qwen2.5-3B-Instruct Q4_K_M
//!   (fichiers sous `tools/models/`, non versionnés).
//!
//! ## Enseignements (11/08/2026)
//!
//! - Le decode CPU Q4_K est quasi parfaitement limité par la bande passante
//!   mesurée : `eff_ram` résolu ≈ 0,99 (3B) ; ≥ 1,0 (0,5B — la L3 96 Mo du
//!   9800X3D sert ~20 % du modèle, effet non modélisé, voir ADR 0002) ;
//! - Le prefill effectif varie de 1,0 TFLOPS (0,5B) à 1,7 TFLOPS (3B) ;
//!   une constante unique à 1,25 TFLOPS donne ≤ ±26 % sur les deux ;
//! - Avec `eff_ram` calibré, l'erreur decode est de −16 % (0,5B) et +0,4 %
//!   (3B) : **gate P0 satisfait sur cet hôte** (< 30 %).

use aos_placement::{
    CostModel, HardwareProfile, ModelDesc, PlacementProfile, PlacementSim, Priority, PrivacyClass,
};

/// Une mesure réelle llama.cpp.
pub struct Measurement {
    pub model: ModelDesc,
    /// tg mesuré (tok/s, decode).
    pub tg_tok_s: f64,
    /// pp mesuré (tok/s, prefill, 128 tokens).
    pub pp_tok_s: f64,
}

/// Ligne de comparaison estimation vs mesure.
#[derive(Debug, Clone)]
pub struct XvalRow {
    pub model_id: String,
    pub measured_tok_s: f64,
    pub estimated_default: f64,
    pub error_default_pct: f64,
    pub estimated_calibrated: f64,
    pub error_calibrated_pct: f64,
    /// eff_ram résolu analytiquement sur cette mesure.
    pub solved_eff_ram: f64,
}

/// Profil matériel de l'hôte de mesure (CPU-only pour llama-bench).
pub fn host_profile() -> HardwareProfile {
    const GIB: u64 = 1 << 30;
    HardwareProfile {
        name: "ryzen-9800x3d-ddr5".into(),
        has_gpu: false,
        vram_total: 0,
        ram_total: 64 * GIB,
        disk_total: 1024 * GIB,
        os_reserve_vram: 0,
        os_reserve_ram: 4 * GIB,
        gpu_mem_bw: 0.0,
        ram_mem_bw: 45.24e9, // mesuré via host_probe (meilleur cas multi-thread)
        disk_seq_bw: 6e9,
        host_to_device_bw: 0.0,
        gpu_flops: 0.0,
        // Réglé pour que flops × eff_prefill_cpu ≈ 1,25 TFLOPS effectifs
        // (médiane des mesures pp 0,5B/3B : 1,0 / 1,7 TFLOPS).
        cpu_flops: 2.5e12,
        gpus: vec![],
        cpu_isa: Default::default(),
        cpu_topology: aos_placement::CpuTopology { logical_cores: 16, performance_cores: 8, efficiency_cores: 0 },
        gpu_backend: Default::default(),
        npu: None,
        webgpu: None,
        remote_nodes: 0,
        thermal: Default::default(),
    }
}

/// Modèles mesurés (descriptions alignées sur les fichiers GGUF utilisés).
pub fn measurements() -> Vec<Measurement> {
    vec![
        Measurement {
            model: ModelDesc {
                id: "qwen2.5-0.5b-q4_k_m".into(),
                name: "Qwen2.5 0.5B Q4_K_M".into(),
                n_layers: 24,
                n_params: 630.17e6,
                weights_bytes: 485_436_211, // 462,96 MiB (taille llama-bench)
                embed_bytes: 76_600_000,
                kv_bytes_per_token: 25_000,
                context_length: 32768,
                supports_layer_offload: true,
                privacy_class: PrivacyClass::Local,
                quantization: Default::default(),
                backends_compatible: vec![],
            },
            tg_tok_s: 112.04,
            pp_tok_s: 808.16,
        },
        Measurement {
            model: ModelDesc {
                id: "qwen2.5-3b-q4_k_m".into(),
                name: "Qwen2.5 3B Q4_K_M".into(),
                n_layers: 36,
                n_params: 3.4e9,
                weights_bytes: 2_093_886_464, // 1,95 GiB (taille llama-bench)
                embed_bytes: 175_000_000,
                kv_bytes_per_token: 37_000,
                context_length: 32768,
                supports_layer_offload: true,
                privacy_class: PrivacyClass::Local,
                quantization: Default::default(),
                backends_compatible: vec![],
            },
            tg_tok_s: 21.22,
            pp_tok_s: 246.93,
        },
    ]
}

/// Coût calibré sur l'hôte : eff_ram résolu ≈ 0,99 (arrondi), le reste
/// hérite des défauts.
pub fn calibrated_cost() -> CostModel {
    CostModel {
        eff_ram: 0.99,
        ..CostModel::default()
    }
}

/// Exécute la comparaison sur toutes les mesures.
pub fn cross_validate() -> Vec<XvalRow> {
    let hw = host_profile();
    let mut rows = Vec::new();
    for m in measurements() {
        let rows_for = |cost: CostModel| {
            let mut sim = PlacementSim::new(hw.clone(), cost);
            sim.place(
                &m.model,
                PlacementProfile::CpuOnly,
                Priority::AgentNormal,
                256,
            )
            .expect("placement cpu-only sur hôte 64 GiB");
            sim.estimate(&m.model.id, 128, 192).unwrap()
        };
        let est_def = rows_for(CostModel::default());
        let est_cal = rows_for(calibrated_cost());
        let solved = CostModel::default().solve_efficiency(
            m.model.weights_bytes as f64,
            hw.ram_mem_bw,
            m.tg_tok_s,
        );
        rows.push(XvalRow {
            model_id: m.model.id.clone(),
            measured_tok_s: m.tg_tok_s,
            estimated_default: est_def.tok_s,
            error_default_pct: (est_def.tok_s - m.tg_tok_s) / m.tg_tok_s * 100.0,
            estimated_calibrated: est_cal.tok_s,
            error_calibrated_pct: (est_cal.tok_s - m.tg_tok_s) / m.tg_tok_s * 100.0,
            solved_eff_ram: solved,
        });
    }
    rows
}

/// Rendu tableau pour le rapport.
pub fn render_rows(rows: &[XvalRow]) -> String {
    let mut out = String::from(
        "modèle                 | mesuré tg | est. défaut | err.   | est. calibré | err.   | eff_ram résolu\n",
    );
    out.push_str(
        "-----------------------+-----------+-------------+--------+--------------+--------+----------------\n",
    );
    for r in rows {
        out.push_str(&format!(
            "{:<22} | {:7.2}  | {:9.2}   | {:+5.1}% | {:10.2}   | {:+5.1}% | {:.2}\n",
            r.model_id,
            r.measured_tok_s,
            r.estimated_default,
            r.error_default_pct,
            r.estimated_calibrated,
            r.error_calibrated_pct,
            r.solved_eff_ram
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Gate P0 : erreur d'estimation < 30 % après étalonnage empirique.
    #[test]
    fn erreur_decode_calibree_sous_30_pourcent() {
        let rows = cross_validate();
        assert_eq!(rows.len(), 2);
        for r in &rows {
            assert!(
                r.error_calibrated_pct.abs() < 30.0,
                "{} : erreur calibrée {:+.1}% (est. {:.2} vs mesuré {:.2})",
                r.model_id,
                r.error_calibrated_pct,
                r.estimated_calibrated,
                r.measured_tok_s
            );
        }
    }

    /// Documente l'écart avant/après étalonnage (le défaut historique
    /// eff_ram=0,45 était 2× trop pessimiste sur cet hôte).
    #[test]
    fn calibration_ameliore_nettement_l_estimation() {
        let rows = cross_validate();
        for r in &rows {
            assert!(r.error_calibrated_pct.abs() < r.error_default_pct.abs());
        }
    }
}
