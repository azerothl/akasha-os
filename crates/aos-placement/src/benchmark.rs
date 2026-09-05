//! Deterministic planning benchmark matrix.
//!
//! This deliberately benchmarks the reproducible planning/cost-model path,
//! not a machine-specific llama.cpp execution. It gives CI a CPU, CUDA-like
//! and Metal-like baseline without requiring an accelerator. Real backend
//! measurements can be compared to the same CSV schema by the caller.

use crate::{CostModel, HardwareProfile, ModelDesc, PlacementManager, PlacementProfile, Priority};

#[derive(Debug, Clone)]
pub struct BenchmarkScenario {
    pub name: &'static str,
    pub hardware: HardwareProfile,
    pub profile: PlacementProfile,
    pub prompt_tokens: u32,
    pub context_tokens: u32,
}

#[derive(Debug, Clone)]
pub struct BenchmarkResult {
    pub scenario: &'static str,
    pub profile: PlacementProfile,
    pub feasible: bool,
    pub ttft_ms: Option<f64>,
    pub decode_tok_s: Option<f64>,
    pub vram_bytes: u64,
    pub ram_bytes: u64,
    pub disk_bytes: u64,
    pub error: Option<String>,
}

/// Fixed fixtures ensure results remain comparable across CI runs. They are
/// named after target classes, not actual installed devices.
pub fn reference_scenarios() -> Vec<BenchmarkScenario> {
    vec![
        BenchmarkScenario {
            name: "cpu",
            hardware: HardwareProfile::cpu_only_laptop(),
            profile: PlacementProfile::CpuOnly,
            prompt_tokens: 256,
            context_tokens: 2048,
        },
        BenchmarkScenario {
            name: "cuda",
            hardware: HardwareProfile::reference_v1(),
            profile: PlacementProfile::Balanced,
            prompt_tokens: 256,
            context_tokens: 2048,
        },
        BenchmarkScenario {
            name: "metal",
            hardware: HardwareProfile::metal_reference(),
            profile: PlacementProfile::Balanced,
            prompt_tokens: 256,
            context_tokens: 2048,
        },
    ]
}

/// Execute a single scenario with no ambient hardware or clock dependency.
pub fn run_scenario(model: &ModelDesc, scenario: &BenchmarkScenario) -> BenchmarkResult {
    let manager = PlacementManager::new(scenario.hardware.clone(), CostModel::default());
    match manager.place_model(
        model,
        scenario.profile,
        Priority::Interactive,
        scenario.context_tokens,
    ) {
        Ok(plan) => {
            let estimate = manager.estimate(
                &plan,
                model,
                scenario.prompt_tokens,
                scenario.context_tokens,
            );
            BenchmarkResult {
                scenario: scenario.name,
                profile: plan.profile,
                feasible: true,
                ttft_ms: Some(estimate.ttft_ms),
                decode_tok_s: Some(estimate.tok_s),
                vram_bytes: plan.bytes_on(crate::Tier::Vram),
                ram_bytes: plan.bytes_on(crate::Tier::Ram),
                disk_bytes: plan.bytes_on(crate::Tier::Disk),
                error: None,
            }
        }
        Err(error) => BenchmarkResult {
            scenario: scenario.name,
            profile: scenario.profile,
            feasible: false,
            ttft_ms: None,
            decode_tok_s: None,
            vram_bytes: 0,
            ram_bytes: 0,
            disk_bytes: 0,
            error: Some(error.to_string()),
        },
    }
}

pub fn run_reference_matrix(model: &ModelDesc) -> Vec<BenchmarkResult> {
    reference_scenarios()
        .iter()
        .map(|scenario| run_scenario(model, scenario))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::model_3b;

    #[test]
    fn reference_matrix_covers_cpu_cuda_and_metal_without_device_access() {
        let results = run_reference_matrix(&model_3b());
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|result| result.feasible));
        let cpu = results
            .iter()
            .find(|result| result.scenario == "cpu")
            .unwrap();
        let cuda = results
            .iter()
            .find(|result| result.scenario == "cuda")
            .unwrap();
        let metal = results
            .iter()
            .find(|result| result.scenario == "metal")
            .unwrap();
        assert_eq!(cpu.vram_bytes, 0);
        assert!(cuda.vram_bytes > 0 && metal.vram_bytes > 0);
        assert!(cuda.decode_tok_s.unwrap() > 0.0 && metal.ttft_ms.unwrap() > 0.0);
    }
}
