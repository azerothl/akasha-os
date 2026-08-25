//! `aos-gate-gemma4-vision` — Gate coordinateur Gemma 4 E4B (texte + vision mtmd).
//!
//! Critères (doivent passer avant merge) :
//! 1. Inférence texte seule sans `ChatTemplate` ;
//! 2. Inférence avec image PNG atteignant `generate_with_images` / mtmd prefill.
//!
//! ## Mode direct (sans bus, recommandé CI / dev)
//!
//! ```text
//! AOS_CPU_ONLY=1 cargo run -p aos-model --bin aos-gate-gemma4-vision --no-default-features -- --direct
//! ```
//!
//! ## Mode bus (`model.infer`, comme Preview)
//!
//! ```text
//! aos-busd &
//! AOS_CPU_ONLY=1 aos-modeld demo/gemma4-gate.yaml &
//! AOS_CPU_ONLY=1 cargo run -p aos-model --bin aos-gate-gemma4-vision --no-default-features
//! ```
//!
//! Variables d'environnement :
//! - `AOS_GEMMA4_WEIGHTS` — GGUF principal (défaut `var/models/gemma4-gate/gemma-4-E4B-it-Q4_K_M.gguf`)
//! - `AOS_GEMMA4_MMPROJ` — sidecar mmproj catalogue (défaut `var/models/gemma4-gate/mmproj-gemma-4-E4B-it-F16.gguf`)
//! - `AOS_GEMMA4_IMAGE` — PNG/JPEG de test (défaut fixture 1×1 intégrée)
//! - `AOS_CPU_ONLY=1` — force CPU (`n_gpu_layers=0`)

use aos_ipc::BusClient;
use aos_llama::{GenParams, LlamaBackend, LlamaContext, LlamaError, LlamaModel, LoadOptions};
use aos_proto::{
    ChatMessage, InferParams, InferRequest, LoadRequest, LoadResponse, TokenEvent,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

struct Gate {
    name: &'static str,
    passed: bool,
    detail: String,
}

fn report(g: &Gate) {
    println!(
        "  {} {} — {}",
        if g.passed { "✓" } else { "✗" },
        g.name,
        g.detail
    );
}

fn default_weights() -> PathBuf {
    std::env::var("AOS_GEMMA4_WEIGHTS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("var/models/gemma4-gate/gemma-4-E4B-it-Q4_K_M.gguf"))
}

fn default_mmproj(weights: &Path) -> PathBuf {
    std::env::var("AOS_GEMMA4_MMPROJ")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            weights
                .parent()
                .map(|d| d.join("mmproj-gemma-4-E4B-it-F16.gguf"))
                .unwrap_or_else(|| PathBuf::from("mmproj-gemma-4-E4B-it-F16.gguf"))
        })
}

fn default_image() -> PathBuf {
    std::env::var("AOS_GEMMA4_IMAGE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/gate-pixel.png")
        })
}

fn cpu_only() -> bool {
    std::env::var("AOS_CPU_ONLY")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn is_chat_template_err(e: &LlamaError) -> bool {
    matches!(e, LlamaError::ChatTemplate)
}

fn gate_direct(weights: &Path, mmproj: &Path, image: &Path) -> Vec<Gate> {
    let mut gates = Vec::new();
    let _backend = LlamaBackend::init();
    let ngl = if cpu_only() { 0 } else { 999 };

    if !weights.is_file() {
        gates.push(Gate {
            name: "prérequis poids",
            passed: false,
            detail: format!("introuvable: {}", weights.display()),
        });
        return gates;
    }
    if !mmproj.is_file() {
        gates.push(Gate {
            name: "prérequis mmproj",
            passed: false,
            detail: format!("introuvable: {}", mmproj.display()),
        });
        return gates;
    }
    if !image.is_file() {
        gates.push(Gate {
            name: "prérequis image PNG",
            passed: false,
            detail: format!("introuvable: {}", image.display()),
        });
        return gates;
    }

    let opts = LoadOptions {
        n_gpu_layers: ngl,
        n_ctx: 4096,
        n_threads: 8,
        mmproj_path: Some(mmproj.to_path_buf()),
        ..Default::default()
    };

    let load_t = Instant::now();
    let model = match LlamaModel::load(weights, &opts) {
        Ok(m) => m,
        Err(e) => {
            gates.push(Gate {
                name: "chargement modèle",
                passed: false,
                detail: e.to_string(),
            });
            return gates;
        }
    };
    gates.push(Gate {
        name: "chargement modèle + mmproj",
        passed: true,
        detail: format!(
            "{:.1?}, {} couches, mmproj ok",
            load_t.elapsed(),
            model.n_layer
        ),
    });

    let mut ctx = match LlamaContext::new(Arc::new(model), &opts) {
        Ok(c) => c,
        Err(e) => {
            gates.push(Gate {
                name: "contexte llama",
                passed: false,
                detail: e.to_string(),
            });
            return gates;
        }
    };

    let params = GenParams {
        max_tokens: 8,
        temperature: 0.2,
        top_p: 0.9,
        seed: 1,
    };
    let messages = vec![("user".to_string(), "Réponds en un mot : couleur du ciel.".to_string())];

    let text_res = {
        let mut out = String::new();
        ctx.generate(&messages, &params, |piece| {
            out.push_str(piece);
            true
        })
    };
    match text_res {
        Ok(stats) if stats.generated_tokens > 0 => {
            gates.push(Gate {
                name: "G1 texte seul (generate)",
                passed: true,
                detail: format!(
                    "prompt={} généré={} stop={:?}",
                    stats.prompt_tokens, stats.generated_tokens, stats.stopped
                ),
            });
        }
        Ok(stats) => {
            gates.push(Gate {
                name: "G1 texte seul (generate)",
                passed: false,
                detail: format!("0 tokens générés, stop={:?}", stats.stopped),
            });
        }
        Err(e) => {
            let chat = is_chat_template_err(&e);
            gates.push(Gate {
                name: "G1 texte seul (generate)",
                passed: false,
                detail: if chat {
                    format!("ChatTemplate: {e}")
                } else {
                    e.to_string()
                },
            });
        }
    }

    if !ctx.has_vision() {
        gates.push(Gate {
            name: "G2 vision mtmd prefill",
            passed: false,
            detail: "has_vision() false — mmproj non chargé".into(),
        });
        return gates;
    }

    let vision_messages =
        vec![("user".to_string(), "Décris l'image en un mot.".to_string())];
    let vision_res = {
        let mut out = String::new();
        ctx.generate_with_images(&vision_messages, &params, &[image], |piece| {
            out.push_str(piece);
            true
        })
    };
    match vision_res {
        Ok(stats) if stats.prompt_tokens > 0 => {
            gates.push(Gate {
                name: "G2 vision (generate_with_images / mtmd)",
                passed: true,
                detail: format!(
                    "prompt_positions={} généré={} stop={:?}",
                    stats.prompt_tokens, stats.generated_tokens, stats.stopped
                ),
            });
        }
        Ok(stats) => {
            gates.push(Gate {
                name: "G2 vision (generate_with_images / mtmd)",
                passed: false,
                detail: format!(
                    "prefill vide? prompt={} généré={}",
                    stats.prompt_tokens, stats.generated_tokens
                ),
            });
        }
        Err(e) => {
            let chat = is_chat_template_err(&e);
            gates.push(Gate {
                name: "G2 vision (generate_with_images / mtmd)",
                passed: false,
                detail: if chat {
                    format!("ChatTemplate avant mtmd: {e}")
                } else {
                    e.to_string()
                },
            });
        }
    }

    gates
}

async fn infer_bus(
    bus: &BusClient,
    model_id: Option<String>,
    prompt: &str,
    images: Vec<String>,
) -> Result<(u32, u32, f64), String> {
    let mut rx = bus
        .call_stream::<InferRequest, TokenEvent>(
            "model.infer",
            &InferRequest {
                model_id,
                messages: vec![ChatMessage {
                    role: "user".into(),
                    content: prompt.into(),
                }],
                params: InferParams {
                    max_tokens: 8,
                    temperature: 0.2,
                    top_p: 0.9,
                    seed: Some(1),
                },
                priority: 3,
                data_refs: vec![],
                images,
                routing: None,
            },
            vec![],
        )
        .await
        .map_err(|e| e.to_string())?;
    let mut n_gen = 0u32;
    let mut n_prompt = 0u32;
    let mut ttft = 0.0;
    while let Some(ev) = rx.recv().await {
        match ev {
            Ok(TokenEvent::Done {
                ttft_ms,
                generated_tokens,
                prompt_tokens,
                ..
            }) => {
                ttft = ttft_ms;
                n_gen = generated_tokens;
                n_prompt = prompt_tokens;
            }
            Ok(TokenEvent::Error { message }) => return Err(message),
            Err(e) => return Err(e.to_string()),
            _ => {}
        }
    }
    Ok((n_prompt, n_gen, ttft))
}

async fn gate_bus(bus_addr: &str, image: &Path) -> Vec<Gate> {
    let mut gates = Vec::new();
    let bus = match BusClient::connect(bus_addr, "gate-gemma4").await {
        Ok(b) => b,
        Err(e) => {
            gates.push(Gate {
                name: "connexion bus",
                passed: false,
                detail: e.to_string(),
            });
            return gates;
        }
    };

    let load = bus
        .call::<LoadRequest, LoadResponse>(
            "model.load",
            &LoadRequest {
                model_id: "local:gemma-4-e4b".into(),
                profile: "cpu-only".into(),
                kv_tokens: 4096,
            },
            vec![],
        )
        .await;
    match load {
        Ok(r) => {
            gates.push(Gate {
                name: "model.load local:gemma-4-e4b",
                passed: true,
                detail: format!("profile={}", r.effective_profile),
            });
        }
        Err(e) => {
            gates.push(Gate {
                name: "model.load local:gemma-4-e4b",
                passed: false,
                detail: e.to_string(),
            });
            return gates;
        }
    }

    let text = infer_bus(&bus, Some("local:gemma-4-e4b".into()), "Un mot : ok", vec![]).await;
    match text {
        Ok((prompt, gen, ttft)) if gen > 0 => {
            gates.push(Gate {
                name: "G1 model.infer texte seul",
                passed: true,
                detail: format!("prompt={} généré={} TTFT={:.0}ms", prompt, gen, ttft),
            });
        }
        Ok((prompt, gen, _)) => {
            gates.push(Gate {
                name: "G1 model.infer texte seul",
                passed: false,
                detail: format!("0 tokens (prompt={})", prompt),
            });
        }
        Err(e) => {
            let chat = e.contains("chat template") || e.contains("ChatTemplate");
            gates.push(Gate {
                name: "G1 model.infer texte seul",
                passed: false,
                detail: if chat {
                    format!("ChatTemplate: {e}")
                } else {
                    e
                },
            });
        }
    }

    if !image.is_file() {
        gates.push(Gate {
            name: "G2 model.infer + image",
            passed: false,
            detail: format!("image introuvable: {}", image.display()),
        });
        return gates;
    }

    let img_path = image.to_string_lossy().into_owned();
    let vision = infer_bus(
        &bus,
        Some("local:gemma-4-e4b".into()),
        "Décris en un mot.",
        vec![img_path],
    )
    .await;
    match vision {
        Ok((prompt, gen, ttft)) if prompt > 0 => {
            gates.push(Gate {
                name: "G2 model.infer + PNG (mtmd)",
                passed: true,
                detail: format!(
                    "prompt_positions={} généré={} TTFT={:.0}ms",
                    prompt, gen, ttft
                ),
            });
        }
        Ok((prompt, gen, _)) => {
            gates.push(Gate {
                name: "G2 model.infer + PNG (mtmd)",
                passed: false,
                detail: format!("prefill? prompt={} généré={}", prompt, gen),
            });
        }
        Err(e) => {
            let chat = e.contains("chat template") || e.contains("ChatTemplate");
            gates.push(Gate {
                name: "G2 model.infer + PNG (mtmd)",
                passed: false,
                detail: if chat {
                    format!("ChatTemplate: {e}")
                } else {
                    e
                },
            });
        }
    }

    gates
}

#[tokio::main]
async fn main() {
    let direct = std::env::args().any(|a| a == "--direct");
    let weights = default_weights();
    let mmproj = default_mmproj(&weights);
    let image = default_image();

    println!("=== Gate Gemma 4 E4B — texte + vision ===");
    println!("  mode: {}", if direct { "direct (aos-llama)" } else { "bus (model.infer)" });
    println!("  weights: {}", weights.display());
    println!("  mmproj:  {}", mmproj.display());
    println!("  image:   {}", image.display());
    println!("  CPU only: {}\n", cpu_only());

    let gates = if direct {
        gate_direct(&weights, &mmproj, &image)
    } else {
        let bus_addr = std::env::args()
            .nth(1)
            .filter(|a| a != "--direct")
            .unwrap_or_else(|| format!("127.0.0.1:{}", aos_ipc::DEFAULT_BUS_PORT));
        gate_bus(&bus_addr, &image).await
    };

    for g in &gates {
        report(g);
    }
    let failed = gates.iter().filter(|g| !g.passed).count();
    println!();
    if failed == 0 {
        println!("GATE GEMMA4 VISION: PASS ({})", gates.len());
        std::process::exit(0);
    } else {
        println!("GATE GEMMA4 VISION: FAIL ({}/{})", failed, gates.len());
        std::process::exit(1);
    }
}
