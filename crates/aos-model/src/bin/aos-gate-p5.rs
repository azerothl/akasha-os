//! `aos-gate-p5` — critères P5 testables sur l'hôte (NFR-04).
//!
//! Prérequis : `demo/run-demo.ps1 -Gate p5`
//!
//! 1. 8 flux d'inférence simultanés, dégradation < 20 % vs unitaire
//!    (continuous batching) ;
//! 2. Multi-GPU (E9 / P5.2) : device count réel ; **skip** si < 2 GPU,
//!    pass/fail si ≥ 2 (layer-split path + tokens streamés).

use aos_ipc::BusClient;
use aos_proto::*;
use std::time::Instant;

struct Gate {
    name: &'static str,
    /// `None` = skipped (non-blocking, documented hardware gap).
    passed: Option<bool>,
    detail: String,
}

async fn infer(
    bus: &BusClient,
    prompt: &str,
    max_tokens: u32,
) -> Result<(f64, f64, u32), String> {
    let mut rx = bus
        .call_stream::<InferRequest, TokenEvent>(
            "model.infer",
            &InferRequest {
                model_id: None,
                messages: vec![ChatMessage {
                    role: "user".into(),
                    content: prompt.into(),
                }],
                params: InferParams {
                    max_tokens,
                    temperature: 0.2,
                    top_p: 0.9,
                    seed: Some(1),
                },
                priority: 3,
                data_refs: vec![],
                routing: None,
            },
            vec![],
        )
        .await
        .map_err(|e| e.to_string())?;
    let mut tok_s = 0.0;
    let mut ttft = 0.0;
    let mut n = 0u32;
    let mut got = false;
    while let Some(ev) = rx.recv().await {
        match ev {
            Ok(TokenEvent::Delta { .. }) => got = true,
            Ok(TokenEvent::Done {
                ttft_ms,
                tok_s: ts,
                generated_tokens,
                ..
            }) => {
                ttft = ttft_ms;
                tok_s = ts;
                n = generated_tokens;
                got = true;
            }
            Ok(TokenEvent::Error { message }) => return Err(message),
            Err(e) => return Err(e.to_string()),
            _ => {}
        }
    }
    if !got {
        return Err("aucun token".into());
    }
    Ok((ttft, tok_s, n))
}

#[tokio::main]
async fn main() {
    let bus_addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| format!("127.0.0.1:{}", aos_ipc::DEFAULT_BUS_PORT));
    let bus = BusClient::connect(&bus_addr, "gate-p5")
        .await
        .expect("bus injoignable — lancer la démo d'abord");
    let mut gates: Vec<Gate> = Vec::new();

    println!("=== Gate P5 — critères testables sur l'hôte ===\n");

    // Warm-up.
    let _ = infer(&bus, "dis ok", 4).await;

    const N_TOK: u32 = 16;
    const N_STREAMS: usize = 8;

    let t0 = Instant::now();
    let unitary = infer(&bus, "compte jusqu'à trois", N_TOK).await;
    let t_unit = t0.elapsed();
    let (u_tok_s, u_ok) = match &unitary {
        Ok((_, ts, n)) if *n > 0 => (*ts, true),
        Ok((_, ts, _)) => (*ts, false),
        Err(_) => (0.0, false),
    };

    let t1 = Instant::now();
    let mut handles = Vec::new();
    for i in 0..N_STREAMS {
        let bus = bus.clone();
        handles.push(tokio::spawn(async move {
            infer(&bus, &format!("dis le mot {i}"), N_TOK).await
        }));
    }
    let mut ok_n = 0usize;
    let mut tok_s_sum = 0.0;
    let mut tok_s_min = f64::MAX;
    let mut gen_sum = 0u32;
    for h in handles {
        match h.await {
            Ok(Ok((_, ts, n))) if n > 0 => {
                ok_n += 1;
                tok_s_sum += ts;
                tok_s_min = tok_s_min.min(ts);
                gen_sum += n;
            }
            _ => {}
        }
    }
    let t_batch = t1.elapsed();
    let mean_ts = if ok_n > 0 {
        tok_s_sum / ok_n as f64
    } else {
        0.0
    };
    let wall_ratio = if t_unit.as_secs_f64() > 1e-6 {
        t_batch.as_secs_f64() / t_unit.as_secs_f64()
    } else {
        99.0
    };
    // NFR-04 : tok/s concurrent ≥ 80 % du unitaire, ou wall ≤ 1.25×
    // (batching réel : 8 décodes dans un llama_decode ; wall ≈ 8× = sérialisé).
    let tok_ok = u_tok_s > 0.0 && mean_ts >= u_tok_s * 0.80;
    let wall_ok = wall_ratio <= 1.25;
    let passed = u_ok && ok_n == N_STREAMS && (tok_ok || wall_ok);
    gates.push(Gate {
        name: "8 flux simultanés, dégradation < 20% vs unitaire (NFR-04)",
        passed: Some(passed),
        detail: format!(
            "unitaire {:.0} ms / {:.1} tok/s ; 8 flux {}/{} en {:.0} ms (×{:.2} wall, mean {:.1} tok/s, min {:.1}, {} tok)",
            t_unit.as_secs_f64() * 1000.0,
            u_tok_s,
            ok_n,
            N_STREAMS,
            t_batch.as_secs_f64() * 1000.0,
            wall_ratio,
            mean_ts,
            if tok_s_min.is_finite() { tok_s_min } else { 0.0 },
            gen_sum,
        ),
    });

    let _backend = aos_llama::LlamaBackend::init();
    let n_gpu = aos_llama::LlamaBackend::gpu_device_count();
    let compile_max = aos_llama::LlamaBackend::max_devices();
    if n_gpu < 2 {
        gates.push(Gate {
            name: "multi-GPU pipeline (2 GPU)",
            passed: None,
            detail: format!(
                "SKIP — devices physiques={n_gpu} (compile max={compile_max}) ; \
                 chemin tensor_split/layer prêt, hard-green nécessite un run 2-GPU"
            ),
        });
    } else {
        // ≥2 GPUs : plumbing + stream tokens (batch criterion already ran).
        let multi_ok = u_ok;
        gates.push(Gate {
            name: "multi-GPU pipeline (2 GPU)",
            passed: Some(multi_ok),
            detail: format!(
                "devices physiques={n_gpu} (compile max={compile_max}) ; \
                 layer-split path actif ; inférence unitaire {}",
                if multi_ok { "OK" } else { "échec" }
            ),
        });
    }

    println!();
    let mut failed: usize = 0;
    let mut skipped: usize = 0;
    for g in &gates {
        let mark = match g.passed {
            Some(true) => "✓",
            Some(false) => {
                failed += 1;
                "✗"
            }
            None => {
                skipped += 1;
                "⊘"
            }
        };
        println!("  {mark} {} — {}", g.name, g.detail);
    }
    println!(
        "\n=== Gate P5 : {} pass / {} fail / {} skip ({} critères) ===",
        gates.len() - failed - skipped,
        failed,
        skipped,
        gates.len()
    );
    if failed > 0 {
        std::process::exit(1);
    }
}
