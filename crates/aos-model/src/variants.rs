//! Select only explicitly configured files; never download or quantize at load time.
use aos_placement::{HardwareProfile, ModelDesc, Quantization, QuantizationMetadata};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
pub struct ModelVariant {
    pub path: PathBuf,
    pub weights_bytes: u64,
    pub embed_bytes: u64,
    pub quantization: QuantizationMetadata,
    /// Identifier of the common calibration workload used for this variant set.
    pub calibration: String,
}

pub fn select(
    variants: &[ModelVariant],
    base: &ModelDesc,
    hw: &HardwareProfile,
    memory_budget: u64,
    min_quality: f32,
) -> Result<Option<(ModelDesc, PathBuf)>, String> {
    if variants.is_empty() {
        return Ok(None);
    }
    if !min_quality.is_finite() || !(0.0..=1.0).contains(&min_quality) {
        return Err("qualité minimale invalide".into());
    }
    let calibration = &variants[0].calibration;
    if calibration.trim().is_empty() || variants.iter().any(|v| &v.calibration != calibration) {
        return Err("les variantes doivent partager une calibration explicite".into());
    }
    let backend = if !hw.has_gpu {
        "cpu"
    } else {
        match hw.gpu_backend {
            aos_placement::GpuBackend::Metal => "metal",
            _ => "cuda",
        }
    };
    let candidate = variants
        .iter()
        .filter(|v| {
            let q = &v.quantization;
            let stable = matches!(
                q.format.as_deref().and_then(Quantization::parse),
                Some(
                    Quantization::F16
                        | Quantization::Q8
                        | Quantization::Q6
                        | Quantization::Q5
                        | Quantization::Q4
                )
            );
            stable
                && v.weights_bytes > 0
                && v.embed_bytes <= v.weights_bytes
                && q.quality_score
                    .is_some_and(|s| s.is_finite() && s >= min_quality && s <= 1.0)
                && q.memory_bytes
                    .is_some_and(|m| m >= v.weights_bytes && m <= memory_budget)
                && q.decode_cost.is_some_and(|c| c.is_finite() && c > 0.0)
                && q.prefill_cost.is_some_and(|c| c.is_finite() && c > 0.0)
                && q.compatible_backends.iter().any(|b| b == backend)
                && q.compatible_isas.iter().all(|isa| match isa.as_str() {
                    "avx2" => hw.cpu_isa.avx2,
                    "avx512" => hw.cpu_isa.avx512,
                    "neon" => hw.cpu_isa.neon,
                    _ => false,
                })
                && std::fs::metadata(&v.path)
                    .is_ok_and(|m| m.is_file() && m.len() == v.weights_bytes)
        })
        .min_by(|a, b| {
            // Quality and resident memory are hard gates, measured latency is the objective.
            let cost = |v: &ModelVariant| {
                v.quantization.decode_cost.unwrap() + v.quantization.prefill_cost.unwrap()
            };
            cost(a)
                .total_cmp(&cost(b))
                .then(a.weights_bytes.cmp(&b.weights_bytes))
                .then(a.path.cmp(&b.path))
        });
    // Variants are optional accelerators for selection, never a new hard
    // dependency. A stale benchmark, a removed file or a tighter memory
    // budget must keep the declared base model loadable.
    let Some(candidate) = candidate else {
        return Ok(None);
    };
    let mut model = base.clone();
    model.weights_bytes = candidate.weights_bytes;
    model.embed_bytes = candidate.embed_bytes;
    model.quantization = candidate.quantization.clone();
    Ok(Some((model, candidate.path.clone())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aos_placement::PrivacyClass;

    fn base() -> ModelDesc {
        ModelDesc {
            id: "local:test".into(),
            name: "Test".into(),
            n_layers: 2,
            n_params: 1.0,
            weights_bytes: 8,
            embed_bytes: 0,
            kv_bytes_per_token: 1,
            context_length: 32,
            supports_layer_offload: true,
            privacy_class: PrivacyClass::Local,
            quantization: QuantizationMetadata::default(),
            backends_compatible: vec![],
        }
    }

    fn variant(path: PathBuf, bytes: u64, format: &str, decode_cost: f32) -> ModelVariant {
        ModelVariant {
            path,
            weights_bytes: bytes,
            embed_bytes: 0,
            calibration: "eval-v1".into(),
            quantization: QuantizationMetadata {
                format: Some(format.into()),
                quality_score: Some(0.9),
                memory_bytes: Some(bytes),
                prefill_cost: Some(1.0),
                decode_cost: Some(decode_cost),
                compatible_backends: vec!["cpu".into()],
                compatible_isas: vec![],
            },
        }
    }

    #[test]
    fn selects_only_existing_calibrated_variant_and_keeps_base_as_fallback() {
        let root = std::env::temp_dir().join(format!("aos-variants-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let slow = root.join("slow.gguf");
        let fast = root.join("fast.gguf");
        std::fs::write(&slow, [1_u8; 8]).unwrap();
        std::fs::write(&fast, [2_u8; 8]).unwrap();
        let variants = vec![
            variant(slow, 8, "Q4_K_M", 2.0),
            variant(fast.clone(), 8, "Q5_K_M", 1.0),
        ];
        let selected = select(
            &variants,
            &base(),
            &HardwareProfile::cpu_only_laptop(),
            32,
            0.85,
        )
        .unwrap()
        .expect("variant");
        assert_eq!(selected.1, fast);
        assert_eq!(selected.0.quantization.format.as_deref(), Some("Q5_K_M"));
        std::fs::remove_dir_all(root).unwrap();

        let missing = vec![variant(PathBuf::from("missing.gguf"), 8, "Q4_K_M", 1.0)];
        assert!(select(
            &missing,
            &base(),
            &HardwareProfile::cpu_only_laptop(),
            32,
            0.85
        )
        .unwrap()
        .is_none());
    }
}
