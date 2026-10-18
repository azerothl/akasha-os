//! Reproducible E9 evidence runner (#435). Exit 77 means insufficient hardware.
//! cargo run -p aos-llama --example multi_gpu_smoke -- /path/model.gguf
use aos_llama::{GenParams, LlamaBackend, LlamaContext, LlamaModel, LoadOptions};
use serde_json::json;
use std::{path::PathBuf, sync::Arc};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("provide a local GGUF path")?;
    let _backend = LlamaBackend::init();
    let devices = LlamaBackend::gpu_device_count();
    if devices < 2 {
        println!(
            "{}",
            json!({"status":"blocked","reason":"two physical ggml GPU devices required","detected_gpu_devices":devices})
        );
        std::process::exit(77);
    }
    for (mode, split) in [("single-gpu", vec![1.0, 0.0]), ("two-gpu", vec![0.5, 0.5])] {
        let opts = LoadOptions {
            n_gpu_layers: 999,
            n_ctx: 2048,
            n_threads: 8,
            tensor_split: split.clone(),
            main_gpu: 0,
            ..Default::default()
        };
        let started = std::time::Instant::now();
        let model = LlamaModel::load(&path, &opts)?;
        if model.n_layer < 2 {
            return Err("choose a model with at least two layers".into());
        }
        let mut ctx = LlamaContext::new(Arc::new(model), &opts)?;
        let mut output = String::new();
        let stats = ctx.generate(
            &[(
                "user".into(),
                "Réponds en un mot : capitale de la France ?".into(),
            )],
            &GenParams {
                max_tokens: 32,
                temperature: 0.2,
                top_p: 0.9,
                seed: 42,
            },
            |piece| {
                output.push_str(piece);
                true
            },
        )?;
        if output.trim().is_empty() || stats.generated_tokens == 0 {
            return Err("generation produced no text".into());
        }
        println!(
            "{}",
            json!({"status":"generated","mode":mode,"detected_gpu_devices":devices,
            "tensor_split":split,"main_gpu":0,"output":output,"prompt_tokens":stats.prompt_tokens,
            "generated_tokens":stats.generated_tokens,"elapsed_ms":started.elapsed().as_millis()})
        );
    }
    // The companion stderr log must show non-zero model buffers on CUDA0 and
    // CUDA1 (or the two actual backend device names) before E9 is called green.
    Ok(())
}
