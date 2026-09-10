//! Deterministic planning benchmark matrix.
//!
//! This deliberately benchmarks the reproducible planning/cost-model path,
//! not a machine-specific llama.cpp execution. It gives CI a CPU, CUDA-like
//! and Metal-like baseline without requiring an accelerator. Real backend
//! measurements can be compared to the same CSV schema by the caller.

use crate::{
    AdaptivePlanner, CostModel, HardwareProfile, LayerPipelineExecutor, LayerPipelineMetrics,
    LayerPipelinePlan, LayerStage, ModelDesc, PlacementManager, PlacementProfile, PlannerOptions,
    Priority, SpeculativeStrategy, ThermalPolicy, WorkloadKind,
};

#[derive(Debug, Clone)]
pub struct BenchmarkScenario {
    pub name: &'static str,
    pub hardware: HardwareProfile,
    pub profile: PlacementProfile,
    pub workload: WorkloadKind,
    pub prompt_tokens: u32,
    pub context_tokens: u32,
    pub concurrency: u32,
}

#[derive(Debug, Clone)]
pub struct BenchmarkResult {
    pub scenario: &'static str,
    pub profile: PlacementProfile,
    pub workload: WorkloadKind,
    pub concurrency: u32,
    pub feasible: bool,
    pub ttft_ms: Option<f64>,
    pub decode_tok_s: Option<f64>,
    pub vram_bytes: u64,
    pub ram_bytes: u64,
    pub disk_bytes: u64,
    pub backend: Option<crate::BackendKind>,
    pub speculative: Option<SpeculativeStrategy>,
    pub thermal_policy: Option<ThermalPolicy>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayerPipelineBenchmarkResult {
    pub layers_executed: u32,
    pub transfers: u32,
    pub cancelled: bool,
    pub output_bytes: usize,
}

struct PipelineProbe;

impl LayerPipelineExecutor for PipelineProbe {
    fn execute_layer(
        &mut self,
        _node_id: &str,
        _layer_index: u32,
        _sequence: u32,
        activation: Vec<u8>,
    ) -> Result<Vec<u8>, String> {
        Ok(activation)
    }
}

/// Vérifie le chemin de pipeline sans dépendre d'un GPU, du réseau ou de
/// l'horloge. Les mesures matérielles réelles peuvent compléter ce résultat.
pub fn run_layer_pipeline_benchmark(
    total_layers: u32,
    stages: Vec<LayerStage>,
    activation_bytes: usize,
) -> Result<LayerPipelineBenchmarkResult, String> {
    if activation_bytes == 0 {
        return Err("activation de benchmark vide".into());
    }
    let plan = LayerPipelinePlan::new(total_layers, stages)?;
    let mut probe = PipelineProbe;
    let (_, metrics): (Vec<u8>, LayerPipelineMetrics) =
        plan.run(&mut probe, vec![0; activation_bytes], 0, || false)?;
    Ok(LayerPipelineBenchmarkResult {
        layers_executed: metrics.layers_executed,
        transfers: metrics.transfers,
        cancelled: metrics.cancelled,
        output_bytes: activation_bytes,
    })
}

/// Fixed fixtures ensure results remain comparable across CI runs. They are
/// named after target classes, not actual installed devices.
pub fn reference_scenarios() -> Vec<BenchmarkScenario> {
    vec![
        BenchmarkScenario {
            name: "cpu",
            hardware: HardwareProfile::cpu_only_laptop(),
            profile: PlacementProfile::CpuOnly,
            workload: WorkloadKind::Chat,
            prompt_tokens: 256,
            context_tokens: 2048,
            concurrency: 1,
        },
        BenchmarkScenario {
            name: "cuda",
            hardware: HardwareProfile::reference_v1(),
            profile: PlacementProfile::Balanced,
            workload: WorkloadKind::Chat,
            prompt_tokens: 256,
            context_tokens: 2048,
            concurrency: 1,
        },
        BenchmarkScenario {
            name: "metal",
            hardware: HardwareProfile::metal_reference(),
            profile: PlacementProfile::Balanced,
            workload: WorkloadKind::Chat,
            prompt_tokens: 256,
            context_tokens: 2048,
            concurrency: 1,
        },
    ]
}

/// Execute a single scenario with no ambient hardware or clock dependency.
pub fn run_scenario(model: &ModelDesc, scenario: &BenchmarkScenario) -> BenchmarkResult {
    let manager = PlacementManager::new(scenario.hardware.clone(), CostModel::default());
    let adaptive = AdaptivePlanner::new(scenario.hardware.clone(), PlannerOptions::default());
    let adaptive_plan = adaptive.select(
        model,
        scenario.profile,
        scenario.workload,
        scenario.context_tokens,
        None,
    );
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
                workload: scenario.workload,
                concurrency: scenario.concurrency,
                feasible: true,
                ttft_ms: Some(estimate.ttft_ms),
                decode_tok_s: Some(estimate.tok_s),
                vram_bytes: plan.bytes_on(crate::Tier::Vram),
                ram_bytes: plan.bytes_on(crate::Tier::Ram),
                disk_bytes: plan.bytes_on(crate::Tier::Disk),
                backend: Some(adaptive_plan.backend),
                speculative: Some(adaptive_plan.speculative),
                thermal_policy: Some(adaptive_plan.thermal_policy),
                error: None,
            }
        }
        Err(error) => BenchmarkResult {
            scenario: scenario.name,
            profile: scenario.profile,
            workload: scenario.workload,
            concurrency: scenario.concurrency,
            feasible: false,
            ttft_ms: None,
            decode_tok_s: None,
            vram_bytes: 0,
            ram_bytes: 0,
            disk_bytes: 0,
            backend: None,
            speculative: None,
            thermal_policy: None,
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

pub fn run_extended_matrix(model: &ModelDesc) -> Vec<BenchmarkResult> {
    extended_scenarios()
        .iter()
        .map(|scenario| run_scenario(model, scenario))
        .collect()
}

/// Scénarios complémentaires du gate adaptatif. Ils exercent la politique de
/// sélection et le modèle de coût sans prétendre mesurer un périphérique absent
/// du processus CI.
pub fn extended_scenarios() -> Vec<BenchmarkScenario> {
    let mut thermal = HardwareProfile::reference_v1();
    thermal.thermal.temperature_c = Some(91.0);
    thermal.thermal.sustained_temperature_c = Some(88.0);
    thermal.thermal.throttling = true;
    vec![
        BenchmarkScenario {
            name: "cuda-long-reasoning",
            hardware: HardwareProfile::reference_v1(),
            profile: PlacementProfile::Latency,
            workload: WorkloadKind::LongReasoning,
            prompt_tokens: 512,
            context_tokens: 8192,
            concurrency: 1,
        },
        BenchmarkScenario {
            name: "cuda-agent-tools",
            hardware: HardwareProfile::reference_v1(),
            profile: PlacementProfile::Balanced,
            workload: WorkloadKind::AgentTools,
            prompt_tokens: 768,
            context_tokens: 8192,
            concurrency: 1,
        },
        BenchmarkScenario {
            name: "cuda-concurrent-batch",
            hardware: HardwareProfile::reference_v1(),
            profile: PlacementProfile::Balanced,
            workload: WorkloadKind::Batch,
            prompt_tokens: 256,
            context_tokens: 4096,
            concurrency: 8,
        },
        BenchmarkScenario {
            name: "cuda-thermal-pressure",
            hardware: thermal,
            profile: PlacementProfile::Latency,
            workload: WorkloadKind::Chat,
            prompt_tokens: 256,
            context_tokens: 2048,
            concurrency: 1,
        },
    ]
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
        assert_eq!(cuda.backend, Some(crate::BackendKind::Cuda));
        assert_eq!(cpu.concurrency, 1);
    }

    #[test]
    fn extended_matrix_covers_agent_batch_and_thermal_policies() {
        let results = run_extended_matrix(&model_3b());
        assert_eq!(results.len(), 4);
        assert!(results.iter().all(|result| result.feasible));
        assert!(results.iter().any(|result| {
            result.workload == WorkloadKind::AgentTools
                && result.speculative == Some(SpeculativeStrategy::PromptLookup)
        }));
        assert!(results
            .iter()
            .any(|result| result.workload == WorkloadKind::Batch && result.concurrency > 1));
        assert!(results.iter().any(|result| {
            result.thermal_policy == Some(ThermalPolicy::Quiet)
                || result.thermal_policy == Some(ThermalPolicy::Balanced)
        }));
    }

    #[test]
    fn benchmark_pipeline_mesure_les_transferts_sans_materiel() {
        let result = run_layer_pipeline_benchmark(
            4,
            vec![
                LayerStage {
                    node_id: "a".into(),
                    first_layer: 0,
                    last_layer: 1,
                },
                LayerStage {
                    node_id: "b".into(),
                    first_layer: 2,
                    last_layer: 3,
                },
            ],
            128,
        )
        .unwrap();
        assert_eq!(result.layers_executed, 4);
        assert_eq!(result.transfers, 1);
        assert_eq!(result.output_bytes, 128);
        assert!(!result.cancelled);
    }
}
