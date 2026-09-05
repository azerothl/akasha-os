//! `cargo run -p aos-placement --example adaptive_benchmark --release`
//! emits a deterministic CSV baseline for CPU/CUDA-like/Metal-like planning.

use aos_placement::{run_reference_matrix, ModelDesc, PrivacyClass, QuantizationMetadata};

fn main() {
    const GIB: u64 = 1 << 30;
    let model = ModelDesc {
        id: "benchmark:3b-q4".into(),
        name: "Benchmark 3B Q4".into(),
        n_layers: 28,
        n_params: 3e9,
        weights_bytes: 2 * GIB,
        embed_bytes: 200_000_000,
        kv_bytes_per_token: 120_000,
        context_length: 8192,
        supports_layer_offload: true,
        privacy_class: PrivacyClass::Local,
        quantization: QuantizationMetadata {
            format: Some("Q4_K_M".into()),
            ..Default::default()
        },
        backends_compatible: vec![],
    };
    println!(
        "scenario,profile,feasible,ttft_ms,decode_tok_s,vram_bytes,ram_bytes,disk_bytes,error"
    );
    for result in run_reference_matrix(&model) {
        println!(
            "{},{:?},{},{},{},{},{},{},{}",
            result.scenario,
            result.profile,
            result.feasible,
            result
                .ttft_ms
                .map(|v| format!("{v:.3}"))
                .unwrap_or_default(),
            result
                .decode_tok_s
                .map(|v| format!("{v:.3}"))
                .unwrap_or_default(),
            result.vram_bytes,
            result.ram_bytes,
            result.disk_bytes,
            result.error.unwrap_or_default().replace(',', ";"),
        );
    }
}
