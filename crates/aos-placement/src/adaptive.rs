//! Adaptive inference planning.
//!
//! This module deliberately contains policy only. It never loads a model and
//! never sends data to another machine; callers apply the returned decision to
//! the existing placement/backend contracts.

use crate::hardware::{GpuBackend, HardwareProfile};
use crate::model::{KvCacheType, ModelDesc};
use crate::plan::PlacementProfile;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BackendKind {
    Cpu,
    Cuda,
    Metal,
    Npu,
    WebGpu,
    Lan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Quantization {
    Unknown,
    F16,
    Q8,
    Q6,
    Q5,
    Q4,
    Q3,
    Q2,
    Mxfp4,
}

impl Quantization {
    pub fn parse(value: &str) -> Option<Self> {
        let v = value.to_ascii_lowercase().replace(['_', '-'], "");
        if v == "mxfp4" {
            Some(Self::Mxfp4)
        } else if v == "f16" || v == "bf16" {
            Some(Self::F16)
        } else if v.starts_with("q6") || v == "int6" {
            Some(Self::Q6)
        } else if v.starts_with("q5") || v == "int5" {
            Some(Self::Q5)
        } else if v.starts_with("q4") || v.starts_with("iq4") || v == "int4" {
            Some(Self::Q4)
        } else if v.starts_with("q3") || v.starts_with("iq3") || v == "int3" {
            Some(Self::Q3)
        } else if v.starts_with("q2") || v.starts_with("iq2") || v == "int2" {
            Some(Self::Q2)
        } else if v.contains("q8") || v == "int8" {
            Some(Self::Q8)
        } else if v.contains("f16") || v.contains("bf16") {
            Some(Self::F16)
        } else {
            None
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::F16 => "f16",
            Self::Q8 => "q8",
            Self::Q6 => "q6",
            Self::Q5 => "q5",
            Self::Q4 => "q4",
            Self::Q3 => "q3",
            Self::Q2 => "q2",
            Self::Mxfp4 => "mxfp4",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WorkloadKind {
    Chat,
    LongReasoning,
    AgentTools,
    Batch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SpeculativeStrategy {
    Disabled,
    PromptLookup,
    DraftModel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThermalPolicy {
    Performance,
    Balanced,
    Quiet,
    AlwaysOn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThermalAction {
    Maintain,
    ReduceBatch,
    MigrateToColderTier,
}

/// Small deterministic controller; platform daemons may feed it fresh probes
/// without coupling the planner to a sensor stack.
#[derive(Debug, Clone, Copy)]
pub struct ThermalController {
    pub policy: ThermalPolicy,
    pub soft_limit_c: f32,
    pub hard_limit_c: f32,
}

impl Default for ThermalController {
    fn default() -> Self {
        Self {
            policy: ThermalPolicy::Balanced,
            soft_limit_c: 78.0,
            hard_limit_c: 88.0,
        }
    }
}

impl ThermalController {
    pub fn action(&self, snapshot: crate::hardware::ThermalSnapshot) -> ThermalAction {
        if snapshot.throttling || snapshot.temperature_c.unwrap_or(0.0) >= self.hard_limit_c {
            ThermalAction::MigrateToColderTier
        } else if snapshot.temperature_c.unwrap_or(0.0) >= self.soft_limit_c {
            ThermalAction::ReduceBatch
        } else {
            ThermalAction::Maintain
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendDescriptor {
    pub kind: BackendKind,
    pub name: String,
    pub quantizations: Vec<Quantization>,
    pub gemm: bool,
    pub gemv: bool,
    pub batching: bool,
    pub kv_cache: bool,
    pub memory_cost_factor: f32,
    pub latency_factor: f32,
    /// 0 experimental, 1 preview, 2 stable.
    pub maturity: u8,
    pub experimental: bool,
    /// Adapter registration is not the same as runtime execution support.
    /// Experimental stubs remain inspectable but cannot be selected yet.
    pub executable: bool,
}

/// Registry is data, so new adapters can be added without changing intents.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BackendRegistry {
    pub backends: Vec<BackendDescriptor>,
}

impl BackendRegistry {
    pub fn for_hardware(hw: &HardwareProfile, allow_experimental: bool) -> Self {
        let mut backends = vec![BackendDescriptor {
            kind: BackendKind::Cpu,
            name: "llama.cpp/cpu".into(),
            quantizations: vec![
                Quantization::F16,
                Quantization::Q8,
                Quantization::Q6,
                Quantization::Q5,
                Quantization::Q4,
            ],
            gemm: true,
            gemv: true,
            batching: true,
            kv_cache: true,
            memory_cost_factor: 1.0,
            latency_factor: 1.0,
            maturity: 2,
            experimental: false,
            executable: true,
        }];
        if hw.has_gpu {
            let (kind, name) = match hw.gpu_backend {
                GpuBackend::Metal => (BackendKind::Metal, "llama.cpp/metal"),
                _ => (BackendKind::Cuda, "llama.cpp/cuda"),
            };
            backends.push(BackendDescriptor {
                kind,
                name: name.into(),
                quantizations: vec![
                    Quantization::F16,
                    Quantization::Q8,
                    Quantization::Q6,
                    Quantization::Q5,
                    Quantization::Q4,
                ],
                gemm: true,
                gemv: true,
                batching: true,
                kv_cache: true,
                memory_cost_factor: 0.95,
                latency_factor: 0.65,
                maturity: 2,
                experimental: false,
                executable: true,
            });
        }
        if allow_experimental {
            if hw.npu.is_some() {
                backends.push(BackendDescriptor {
                    kind: BackendKind::Npu,
                    name: "npu/adapter".into(),
                    quantizations: vec![Quantization::Q8, Quantization::Q4],
                    gemm: true,
                    gemv: true,
                    batching: false,
                    kv_cache: false,
                    memory_cost_factor: 0.8,
                    latency_factor: 0.9,
                    maturity: 0,
                    experimental: true,
                    executable: false,
                });
            }
            if hw.webgpu.is_some() {
                backends.push(BackendDescriptor {
                    kind: BackendKind::WebGpu,
                    name: "webgpu/adapter".into(),
                    quantizations: vec![Quantization::F16, Quantization::Q4],
                    gemm: true,
                    gemv: true,
                    batching: true,
                    kv_cache: true,
                    memory_cost_factor: 0.9,
                    latency_factor: 0.85,
                    maturity: 0,
                    experimental: true,
                    executable: false,
                });
            }
        }
        Self { backends }
    }

    pub fn get(&self, kind: BackendKind) -> Option<&BackendDescriptor> {
        self.backends.iter().find(|b| b.kind == kind)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferencePlan {
    pub backend: BackendKind,
    pub quantization: Quantization,
    pub placement: PlacementProfile,
    pub kv_cache: KvCacheType,
    pub kv_tokens: u32,
    pub speculative: SpeculativeStrategy,
    pub thermal_policy: ThermalPolicy,
    pub power_budget_w: Option<f32>,
    pub reason: String,
    pub fallback: Vec<BackendKind>,
    /// Set only when the loader had to apply one of `fallback` backends.
    #[serde(default)]
    pub fallback_used: bool,
    pub experimental: bool,
}

/// Read-only report used by `model.plan`; placement feasibility is filled by
/// the caller because only the Model Subsystem owns current allocations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferencePlanDiagnostic {
    pub requested_profile: PlacementProfile,
    pub plan: InferencePlan,
    pub feasible: Option<bool>,
    pub placement_summary: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct PlannerOptions {
    pub allow_experimental: bool,
    pub min_quality: f32,
    pub speculation: bool,
    pub thermal_policy: ThermalPolicy,
}

impl Default for PlannerOptions {
    fn default() -> Self {
        Self {
            allow_experimental: false,
            min_quality: 0.85,
            speculation: true,
            thermal_policy: ThermalPolicy::Balanced,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AdaptivePlanner {
    pub hardware: HardwareProfile,
    pub registry: BackendRegistry,
    pub options: PlannerOptions,
}

impl AdaptivePlanner {
    pub fn new(hardware: HardwareProfile, options: PlannerOptions) -> Self {
        let registry = BackendRegistry::for_hardware(&hardware, options.allow_experimental);
        Self {
            hardware,
            registry,
            options,
        }
    }

    pub fn select(
        &self,
        model: &ModelDesc,
        requested_profile: PlacementProfile,
        workload: WorkloadKind,
        kv_tokens: u32,
        forced_backend: Option<BackendKind>,
    ) -> InferencePlan {
        let preferred = forced_backend
            .filter(|kind| {
                self.registry
                    .get(*kind)
                    .is_some_and(|backend| backend.executable)
            })
            .or_else(|| {
                if requested_profile == PlacementProfile::CpuOnly {
                    return Some(BackendKind::Cpu);
                }
                self.registry
                    .backends
                    .iter()
                    .find(|b| {
                        b.kind != BackendKind::Cpu
                            && b.executable
                            && model_supports_backend(model, b.kind)
                    })
                    .map(|b| b.kind)
            });
        let backend = preferred
            .filter(|b| self.registry.get(*b).is_some())
            .unwrap_or(BackendKind::Cpu);
        let descriptor = self
            .registry
            .get(backend)
            .expect("CPU backend is always registered");
        let quantization = self.choose_quantization(model, descriptor);
        let hot = self.hardware.thermal.temperature_c.unwrap_or(0.0);
        let thermal_policy = if self.hardware.thermal.throttling || hot >= 85.0 {
            ThermalPolicy::Quiet
        } else {
            self.options.thermal_policy
        };
        let placement = if backend == BackendKind::Cpu {
            PlacementProfile::CpuOnly
        } else if thermal_policy == ThermalPolicy::Quiet {
            PlacementProfile::MemorySaver
        } else {
            requested_profile
        };
        let speculative = if self.options.speculation
            && !matches!(workload, WorkloadKind::Batch | WorkloadKind::Chat)
            && model.n_layers > 0
        {
            SpeculativeStrategy::PromptLookup
        } else {
            SpeculativeStrategy::Disabled
        };
        let fallback = self
            .registry
            .backends
            .iter()
            .filter_map(|b| {
                (b.kind != backend && b.kind == BackendKind::Cpu && b.executable).then_some(b.kind)
            })
            .collect();
        let reason = format!(
            "{}: {} / {} / {:?} ({} tokens)",
            if forced_backend.is_some() {
                "forcé"
            } else {
                "automatique"
            },
            descriptor.name,
            quantization.as_str(),
            workload,
            kv_tokens
        );
        InferencePlan {
            backend,
            quantization,
            placement,
            kv_cache: if backend == BackendKind::Cpu {
                KvCacheType::F16
            } else {
                KvCacheType::Q8_0
            },
            kv_tokens,
            speculative,
            thermal_policy,
            power_budget_w: self.hardware.thermal.power_w,
            reason,
            fallback,
            fallback_used: false,
            experimental: descriptor.experimental,
        }
    }

    pub fn compare_profiles(&self, model: &ModelDesc, kv_tokens: u32) -> Vec<InferencePlan> {
        [
            PlacementProfile::Latency,
            PlacementProfile::Balanced,
            PlacementProfile::MemorySaver,
            PlacementProfile::CpuOnly,
        ]
        .into_iter()
        .map(|profile| self.select(model, profile, WorkloadKind::Chat, kv_tokens, None))
        .collect()
    }

    fn choose_quantization(&self, model: &ModelDesc, _backend: &BackendDescriptor) -> Quantization {
        // A placement decision cannot change the tensors in an existing file.
        model
            .quantization
            .format
            .as_deref()
            .and_then(Quantization::parse)
            .unwrap_or(Quantization::Unknown)
    }
}

fn model_supports_backend(model: &ModelDesc, backend: BackendKind) -> bool {
    if model.backends_compatible.is_empty() {
        return true;
    }
    let names = model
        .backends_compatible
        .iter()
        .map(|name| name.to_ascii_lowercase())
        .collect::<Vec<_>>();
    match backend {
        BackendKind::Cpu | BackendKind::Cuda | BackendKind::Metal => names
            .iter()
            .any(|name| name == "llamacpp" || name == "llama.cpp"),
        BackendKind::Npu => names.iter().any(|name| name == "npu"),
        BackendKind::WebGpu => names.iter().any(|name| name == "webgpu"),
        BackendKind::Lan => names.iter().any(|name| name == "lan"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::model_3b;

    #[test]
    fn cpu_profile_never_selects_gpu() {
        let planner = AdaptivePlanner::new(
            HardwareProfile::cpu_only_laptop(),
            PlannerOptions::default(),
        );
        let plan = planner.select(
            &model_3b(),
            PlacementProfile::Balanced,
            WorkloadKind::Chat,
            2048,
            None,
        );
        assert_eq!(plan.backend, BackendKind::Cpu);
        assert_eq!(plan.placement, PlacementProfile::CpuOnly);
        assert_eq!(plan.kv_cache, KvCacheType::F16);
    }

    #[test]
    fn experimental_adapters_are_opt_in() {
        let mut hw = HardwareProfile::reference_v1();
        hw.npu = Some(crate::NpuCapabilities {
            name: "test".into(),
            memory_bytes: 1,
            int8: true,
            experimental: true,
        });
        assert!(AdaptivePlanner::new(hw.clone(), PlannerOptions::default())
            .registry
            .get(BackendKind::Npu)
            .is_none());
        let mut options = PlannerOptions::default();
        options.allow_experimental = true;
        assert!(AdaptivePlanner::new(hw, options)
            .registry
            .get(BackendKind::Npu)
            .is_some());
    }

    #[test]
    fn metal_and_thermal_policy_are_selected_from_profile() {
        let mut hw = HardwareProfile::metal_reference();
        hw.thermal = crate::ThermalSnapshot {
            temperature_c: Some(91.0),
            sustained_temperature_c: Some(88.0),
            throttling: true,
            power_w: Some(18.0),
        };
        let planner = AdaptivePlanner::new(hw, PlannerOptions::default());
        let plan = planner.select(
            &model_3b(),
            PlacementProfile::Latency,
            WorkloadKind::Chat,
            2048,
            None,
        );
        assert_eq!(plan.backend, BackendKind::Metal);
        assert_eq!(plan.thermal_policy, ThermalPolicy::Quiet);
        assert_eq!(plan.placement, PlacementProfile::MemorySaver);
        assert_eq!(plan.power_budget_w, Some(18.0));
    }

    #[test]
    fn existing_weights_are_never_relabelled_as_another_quantization() {
        let mut model = model_3b();
        model.quantization.format = Some("Q2_K".into());
        let planner =
            AdaptivePlanner::new(HardwareProfile::reference_v1(), PlannerOptions::default());
        let plan = planner.select(
            &model,
            PlacementProfile::Balanced,
            WorkloadKind::Chat,
            2048,
            None,
        );
        assert_eq!(plan.quantization, Quantization::Q2);
        assert_eq!(Quantization::parse("F16"), Some(Quantization::F16));
        assert_eq!(Quantization::parse("IQ4_XS"), Some(Quantization::Q4));
        assert_eq!(Quantization::parse("Qwen2.5 3B"), None);
    }
}
