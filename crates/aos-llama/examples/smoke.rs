//! Smoke test FFI : charge un vrai GGUF et génère quelques tokens.
//!
//! `cargo run -p aos-llama --example smoke --release -- <path.gguf> [ngl]`

use aos_llama::{GenParams, LlamaBackend, LlamaContext, LlamaModel, LoadOptions};
use std::path::PathBuf;
use std::sync::Arc;

fn main() {
    let path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("tools/models/qwen2.5-0.5b-instruct-q4_k_m.gguf"));
    let ngl: i32 = std::env::args()
        .nth(2)
        .and_then(|a| a.parse().ok())
        .unwrap_or(999); // tout sur GPU par défaut si possible

    let _backend = LlamaBackend::init();
    println!(
        "gpu_offload={} devices={}",
        LlamaBackend::supports_gpu_offload(),
        LlamaBackend::max_devices()
    );

    let opts = LoadOptions {
        n_gpu_layers: ngl,
        n_ctx: 2048,
        n_threads: 8,
        ..Default::default()
    };
    let t0 = std::time::Instant::now();
    let model = LlamaModel::load(&path, &opts).expect("chargement modèle");
    println!(
        "modèle chargé en {:.1?} : {} couches, {:.2} Mio",
        t0.elapsed(),
        model.n_layer,
        model.size_bytes as f64 / (1 << 20) as f64
    );

    let mut ctx = LlamaContext::new(Arc::new(model), &opts).expect("contexte");
    let params = GenParams {
        max_tokens: 32,
        temperature: 0.2,
        top_p: 0.9,
        seed: 42,
    };
    let messages = vec![(
        "user".to_string(),
        "Réponds en un seul mot : quelle est la capitale de la France ?".to_string(),
    )];
    let mut text = String::new();
    let stats = ctx
        .generate(&messages, &params, |piece| {
            text.push_str(piece);
            true
        })
        .expect("génération");
    println!("---\nsortie: {text}");
    println!(
        "stats: prompt={} généré={} TTFT={:.0} ms tok/s={:.1} stop={:?}",
        stats.prompt_tokens, stats.generated_tokens, stats.ttft_ms, stats.tok_s, stats.stopped
    );
}
