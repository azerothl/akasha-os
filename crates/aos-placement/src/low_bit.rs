//! Experimental registry and gate for specialized low-bit kernels.
//!
//! This is deliberately a contract, not a promise that a CPU/GPU kernel is
//! available.  A Q2/MXFP4 path is eligible only when a backend registers a
//! compatible kernel and a local benchmark proves a positive gain over INT4.

use crate::adaptive::{BackendKind, Quantization};
use crate::hardware::HardwareProfile;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LowBitLayout {
    Int2,
    Mxfp4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum KernelPhase {
    PrefillGemm,
    DecodeGemv,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LowBitKernel {
    pub backend: BackendKind,
    pub layout: LowBitLayout,
    pub phase: KernelPhase,
    /// Optional ISA name (`avx2`, `avx512`, `neon`) or GPU capability.
    #[serde(default)]
    pub isa: Option<String>,
    pub measured_tokens_per_second: f64,
    pub int4_tokens_per_second: f64,
}

impl LowBitKernel {
    pub fn measured_gain(&self) -> Option<f64> {
        if !self.measured_tokens_per_second.is_finite()
            || !self.int4_tokens_per_second.is_finite()
            || self.measured_tokens_per_second <= 0.0
            || self.int4_tokens_per_second <= 0.0
        {
            return None;
        }
        Some(self.measured_tokens_per_second / self.int4_tokens_per_second)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LowBitKernelRegistry {
    pub kernels: Vec<LowBitKernel>,
}

impl LowBitKernelRegistry {
    /// A 5% margin avoids selecting a noisy low-bit benchmark that is merely
    /// equal to INT4. The caller still owns quality and memory gates.
    pub const MIN_GAIN_VS_INT4: f64 = 1.05;

    pub fn register(&mut self, kernel: LowBitKernel) -> Result<(), String> {
        let gain = kernel
            .measured_gain()
            .ok_or("mesures kernel bas-bit invalides")?;
        if gain < Self::MIN_GAIN_VS_INT4 {
            return Err("kernel bas-bit sans gain mesuré suffisant".into());
        }
        self.kernels.push(kernel);
        Ok(())
    }

    pub fn eligible(
        &self,
        quantization: Quantization,
        backend: BackendKind,
        phase: KernelPhase,
        hw: &HardwareProfile,
    ) -> bool {
        let layout = match quantization {
            Quantization::Q2 => LowBitLayout::Int2,
            Quantization::Mxfp4 => LowBitLayout::Mxfp4,
            _ => return false,
        };
        self.kernels.iter().any(|kernel| {
            kernel.backend == backend
                && kernel.layout == layout
                && kernel.phase == phase
                && kernel
                    .measured_gain()
                    .is_some_and(|gain| gain >= Self::MIN_GAIN_VS_INT4)
                && kernel_isa_matches(kernel.isa.as_deref(), hw)
        })
    }
}

fn kernel_isa_matches(isa: Option<&str>, hw: &HardwareProfile) -> bool {
    match isa.map(str::to_ascii_lowercase).as_deref() {
        None => true,
        Some("avx2") => hw.cpu_isa.avx2,
        Some("avx512") => hw.cpu_isa.avx512,
        Some("neon") => hw.cpu_isa.neon,
        Some("cuda") => hw.has_gpu && hw.gpu_backend == crate::hardware::GpuBackend::Cuda,
        Some("metal") => hw.has_gpu && hw.gpu_backend == crate::hardware::GpuBackend::Metal,
        Some(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kernel(speed: f64) -> LowBitKernel {
        LowBitKernel {
            backend: BackendKind::Cpu,
            layout: LowBitLayout::Int2,
            phase: KernelPhase::DecodeGemv,
            isa: None,
            measured_tokens_per_second: speed,
            int4_tokens_per_second: 100.0,
        }
    }

    #[test]
    fn low_bit_requires_a_real_gain_over_int4() {
        let mut registry = LowBitKernelRegistry::default();
        assert!(registry.register(kernel(104.0)).is_err());
        registry.register(kernel(110.0)).unwrap();
        assert!(registry.eligible(
            Quantization::Q2,
            BackendKind::Cpu,
            KernelPhase::DecodeGemv,
            &HardwareProfile::cpu_only_laptop(),
        ));
    }

    #[test]
    fn unsupported_quantization_never_uses_low_bit_registry() {
        let mut registry = LowBitKernelRegistry::default();
        registry.register(kernel(110.0)).unwrap();
        assert!(!registry.eligible(
            Quantization::Q4,
            BackendKind::Cpu,
            KernelPhase::DecodeGemv,
            &HardwareProfile::cpu_only_laptop(),
        ));
    }
}
