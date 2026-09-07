//! Runtime contracts for optional inference adapters.
//!
//! Registration and execution are intentionally separate.  An advertised NPU
//! or WebGPU device is not enough to make an inference backend executable: a
//! compatible runtime must also be present.  This module gives the planner a
//! small, serializable answer that callers can expose in diagnostics.

use crate::adaptive::BackendKind;
use crate::hardware::HardwareProfile;
use serde::{Deserialize, Serialize};

pub const ADAPTER_PROTOCOL_VERSION: u16 = 1;

/// Capabilities returned by an independently managed NPU/WebGPU runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterHandshake {
    pub protocol_version: u16,
    pub backend: BackendKind,
    pub device: String,
    pub memory_bytes: u64,
    #[serde(default)]
    pub supported_operations: Vec<String>,
    #[serde(default)]
    pub supported_quantizations: Vec<crate::adaptive::Quantization>,
}

pub fn validate_handshake(
    expected_backend: BackendKind,
    expected_memory_bytes: u64,
    expected_operations: &[String],
    expected_quantizations: &[crate::adaptive::Quantization],
    handshake: &AdapterHandshake,
) -> Result<(), String> {
    if handshake.protocol_version != ADAPTER_PROTOCOL_VERSION {
        return Err(format!(
            "version protocole adaptateur incompatible: {}",
            handshake.protocol_version
        ));
    }
    if handshake.backend != expected_backend {
        return Err("backend annoncé différent du backend demandé".into());
    }
    if handshake.device.trim().is_empty() || handshake.memory_bytes == 0 {
        return Err("handshake adaptateur sans périphérique ou mémoire valide".into());
    }
    if expected_memory_bytes > 0 && handshake.memory_bytes < expected_memory_bytes {
        return Err("mémoire runtime inférieure à celle annoncée par la sonde".into());
    }
    if expected_operations
        .iter()
        .any(|op| !contains_case_insensitive(&handshake.supported_operations, op))
    {
        return Err("opération requise absente du runtime adaptateur".into());
    }
    if expected_quantizations
        .iter()
        .any(|quant| !handshake.supported_quantizations.contains(quant))
    {
        return Err("format de quantification requis absent du runtime adaptateur".into());
    }
    Ok(())
}

fn contains_case_insensitive(values: &[String], expected: &str) -> bool {
    values
        .iter()
        .any(|value| value.eq_ignore_ascii_case(expected))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AdapterState {
    Available,
    Unavailable,
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterStatus {
    pub backend: BackendKind,
    pub state: AdapterState,
    pub reason: String,
    #[serde(default)]
    pub fallback: Vec<BackendKind>,
}

impl AdapterStatus {
    pub fn available(backend: BackendKind, reason: impl Into<String>) -> Self {
        Self {
            backend,
            state: AdapterState::Available,
            reason: reason.into(),
            fallback: Vec::new(),
        }
    }

    pub fn unavailable(
        backend: BackendKind,
        reason: impl Into<String>,
        fallback: Vec<BackendKind>,
    ) -> Self {
        Self {
            backend,
            state: AdapterState::Unavailable,
            reason: reason.into(),
            fallback,
        }
    }

    pub fn executable(&self) -> bool {
        self.state == AdapterState::Available
    }
}

/// Probe the stable adapter contract.  NPU and WebGPU remain deliberately
/// unavailable until their native runtimes are integrated; they therefore
/// fail explicitly and can never block the local CPU/GPU path.
pub fn probe(kind: BackendKind, hw: &HardwareProfile, allow_experimental: bool) -> AdapterStatus {
    match kind {
        BackendKind::Cpu => AdapterStatus::available(BackendKind::Cpu, "runtime CPU disponible"),
        BackendKind::Cuda if hw.has_gpu && hw.gpu_backend == crate::hardware::GpuBackend::Cuda => {
            AdapterStatus::available(BackendKind::Cuda, "runtime CUDA disponible")
        }
        BackendKind::Metal if hw.has_gpu && hw.gpu_backend == crate::hardware::GpuBackend::Metal => {
            AdapterStatus::available(BackendKind::Metal, "runtime Metal disponible")
        }
        BackendKind::Cuda | BackendKind::Metal => AdapterStatus::unavailable(
            kind,
            "GPU détecté mais runtime backend incompatible ou absent",
            vec![BackendKind::Cpu],
        ),
        BackendKind::Lan if !allow_experimental => AdapterStatus {
            backend: BackendKind::Lan,
            state: AdapterState::Disabled,
            reason: "adaptateur LAN expérimental désactivé".into(),
            fallback: vec![BackendKind::Cpu],
        },
        BackendKind::Lan if hw.remote_nodes > 0 => AdapterStatus::available(
            BackendKind::Lan,
            "transport LAN Akasha authentifié disponible pour des nœuds appairés",
        ),
        BackendKind::Lan => AdapterStatus::unavailable(
            BackendKind::Lan,
            "aucun nœud LAN appairé",
            vec![BackendKind::Cpu],
        ),
        BackendKind::Npu if !allow_experimental => AdapterStatus {
            backend: BackendKind::Npu,
            state: AdapterState::Disabled,
            reason: "adaptateur NPU expérimental désactivé".into(),
            fallback: vec![BackendKind::Cuda, BackendKind::Metal, BackendKind::Cpu],
        },
        BackendKind::Npu if hw.npu.is_some() => {
            let npu = hw.npu.as_ref().expect("checked above");
            let complete = npu.memory_bytes > 0
                && !npu.supported_operations.is_empty()
                && !npu.supported_quantizations.is_empty();
            let reason = if complete {
                format!(
                    "NPU {} détecté ({} opérations, {} formats), runtime d'exécution Akasha non attaché",
                    npu.name,
                    npu.supported_operations.len(),
                    npu.supported_quantizations.len()
                )
            } else {
                "NPU détecté, mais sa description runtime est incomplète (opérations/formats)".into()
            };
            AdapterStatus::unavailable(
                BackendKind::Npu,
                reason,
                vec![BackendKind::Cuda, BackendKind::Metal, BackendKind::Cpu],
            )
        }
        BackendKind::Npu => AdapterStatus::unavailable(
            BackendKind::Npu,
            "aucun NPU détecté",
            vec![BackendKind::Cuda, BackendKind::Metal, BackendKind::Cpu],
        ),
        BackendKind::WebGpu if !allow_experimental => AdapterStatus {
            backend: BackendKind::WebGpu,
            state: AdapterState::Disabled,
            reason: "adaptateur WebGPU expérimental désactivé".into(),
            fallback: vec![BackendKind::Cuda, BackendKind::Metal, BackendKind::Cpu],
        },
        BackendKind::WebGpu if hw.webgpu.is_some() => {
            let webgpu = hw.webgpu.as_ref().expect("checked above");
            let complete = webgpu.memory_bytes > 0
                && !webgpu.supported_operations.is_empty()
                && !webgpu.supported_quantizations.is_empty();
            let reason = if complete {
                format!(
                    "adaptateur WebGPU {} détecté ({} opérations, {} formats), runtime navigateur non attaché",
                    webgpu.adapter,
                    webgpu.supported_operations.len(),
                    webgpu.supported_quantizations.len()
                )
            } else {
                "WebGPU détecté, mais sa description runtime est incomplète (opérations/formats)".into()
            };
            AdapterStatus::unavailable(
                BackendKind::WebGpu,
                reason,
                vec![BackendKind::Cuda, BackendKind::Metal, BackendKind::Cpu],
            )
        }
        BackendKind::WebGpu => AdapterStatus::unavailable(
            BackendKind::WebGpu,
            "aucun adaptateur WebGPU détecté",
            vec![BackendKind::Cuda, BackendKind::Metal, BackendKind::Cpu],
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn experimental_adapters_fail_explicitly_and_keep_cpu_fallback() {
        let mut hw = HardwareProfile::cpu_only_laptop();
        hw.npu = Some(crate::hardware::NpuCapabilities {
            name: "test-npu".into(),
            memory_bytes: 1024,
            int8: true,
            experimental: true,
            supported_operations: vec!["gemm".into(), "gemv".into()],
            supported_quantizations: vec![crate::Quantization::Q8],
            runtime_endpoint: None,
        });
        let status = probe(BackendKind::Npu, &hw, true);
        assert_eq!(status.state, AdapterState::Unavailable);
        assert!(status.fallback.contains(&BackendKind::Cpu));
        assert!(!status.executable());
    }

    #[test]
    fn incomplete_capability_probe_is_not_treated_as_executable() {
        let mut hw = HardwareProfile::cpu_only_laptop();
        hw.webgpu = Some(crate::hardware::WebGpuCapabilities {
            adapter: "test-webgpu".into(),
            memory_bytes: 512,
            shader_f16: true,
            experimental: true,
            supported_operations: vec![],
            supported_quantizations: vec![],
            runtime_endpoint: None,
        });
        let status = probe(BackendKind::WebGpu, &hw, true);
        assert_eq!(status.state, AdapterState::Unavailable);
        assert!(status.reason.contains("incomplète"));
        assert!(status.fallback.contains(&BackendKind::Cpu));
    }

    #[test]
    fn paired_lan_is_available_only_when_the_gate_is_enabled() {
        let mut hw = HardwareProfile::cpu_only_laptop();
        hw.remote_nodes = 1;
        assert_eq!(probe(BackendKind::Lan, &hw, false).state, AdapterState::Disabled);
        assert!(probe(BackendKind::Lan, &hw, true).executable());
    }

    #[test]
    fn handshake_requires_matching_capabilities() {
        let expected_ops = vec!["gemm".into(), "gemv".into()];
        let expected_quantizations = vec![crate::Quantization::Q4];
        let handshake = AdapterHandshake {
            protocol_version: ADAPTER_PROTOCOL_VERSION,
            backend: BackendKind::Npu,
            device: "test-npu".into(),
            memory_bytes: 2048,
            supported_operations: vec!["GEMM".into(), "gemv".into()],
            supported_quantizations: vec![crate::Quantization::Q4],
        };
        assert!(validate_handshake(
            BackendKind::Npu,
            1024,
            &expected_ops,
            &expected_quantizations,
            &handshake
        )
        .is_ok());
        let mut incompatible = handshake.clone();
        incompatible.backend = BackendKind::WebGpu;
        assert!(validate_handshake(
            BackendKind::Npu,
            1024,
            &expected_ops,
            &expected_quantizations,
            &incompatible
        )
        .is_err());
    }
}
