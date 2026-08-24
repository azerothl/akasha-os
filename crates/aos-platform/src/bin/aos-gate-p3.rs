//! `aos-gate-p3` — vérification exécutable du Gate P3.
//!
//! Prérequis : services démarrés avec `demo/run-demo.ps1 -Gate p3`
//! (platformd en `confirm_timeout_sec: 3` pour le test de timeout).
//!
//! Critères :
//! 1. donnée `secret` → routée local même avec backend distant configuré ;
//! 2. mode `local_only` : aucun egress vers le backend (journal vérifié) ;
//! 3. `fs.delete` → confirmation bloquante ; timeout → refus audité ;
//! 4. trust élevé → cap accordée sans confirmation ; trust faible → refus.

use aos_ipc::BusClient;
use aos_proto::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

struct Gate {
    name: &'static str,
    passed: bool,
    detail: String,
}

/// Mock d'un backend OpenAI-compatible (SSE) sur 127.0.0.1.
async fn start_mock() -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let hits = Arc::new(AtomicUsize::new(0));
    let h2 = hits.clone();
    tokio::spawn(async move {
        while let Ok((mut sock, _)) = listener.accept().await {
            h2.fetch_add(1, Ordering::SeqCst);
            tokio::spawn(async move {
                let mut buf = [0u8; 8192];
                let _ = sock.read(&mut buf).await;
                let body = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n\
                    data: {\"choices\":[{\"delta\":{\"content\":\"Bonjour\"}}]}\n\n\
                    data: {\"choices\":[{\"delta\":{\"content\":\" du\"}}]}\n\n\
                    data: {\"choices\":[{\"delta\":{\"content\":\" mock\"}}]}\n\n\
                    data: [DONE]\n\n";
                let _ = sock.write_all(body.as_bytes()).await;
                let _ = sock.shutdown().await;
            });
        }
    });
    (format!("http://{addr}/v1"), hits)
}

#[tokio::main]
async fn main() {
    let bus_addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| format!("127.0.0.1:{}", aos_ipc::DEFAULT_BUS_PORT));
    let bus = BusClient::connect(&bus_addr, "gate-p3")
        .await
        .expect("bus injoignable — lancer la démo d'abord");
    let mut gates: Vec<Gate> = Vec::new();

    println!("=== Gate P3 — vérification exécutable ===\n");

    // Backend distant mock.
    let (endpoint, mock_hits) = start_mock().await;
    let _ = bus
        .call::<BackendAddRequest, bool>(
            "model.backend.add",
            &BackendAddRequest {
                model_id: "remote:mock:gpt-x".into(),
                endpoint: endpoint.clone(),
                secret_name: None,
                remote_model: Some("mock-model".into()),
            },
            vec![],
        )
        .await;

    // --- 1. Donnée secret → routée local ---
    let secret_path = "/home/u/secrets/gate-secret.md";
    let _ = bus
        .call::<FsWriteRequest, u64>(
            "fs.write",
            &FsWriteRequest {
                path: secret_path.into(),
                content: "donnée classée secret".into(),
                tx_id: None,
                actor: "human:ui".into(),
                caps: vec!["fs.write:/home/**".into()],
                trace_id: String::new(),
            },
            vec![],
        )
        .await;
    let _class = bus
        .call::<FsClassRequest, FsClassResponse>(
            "fs.class",
            &FsClassRequest {
                path: secret_path.into(),
            },
            vec![],
        )
        .await;
    let hits_before = mock_hits.load(Ordering::SeqCst);
    let infer = bus
        .call_stream::<InferRequest, TokenEvent>(
            "model.infer",
            &InferRequest {
                model_id: Some("remote:mock:gpt-x".into()),
                messages: vec![ChatMessage {
                    role: "user".into(),
                    content: "dis bonjour".into(),
                }],
                params: InferParams {
                    max_tokens: 8,
                    ..Default::default()
                },
                priority: 3,
                data_refs: vec![secret_path.into()],
                images: vec![],
                routing: None,
            },
            vec![],
        )
        .await;
    let mut got_tokens = false;
    if let Ok(mut rx) = infer {
        while let Some(ev) = rx.recv().await {
            if matches!(
                ev,
                Ok(TokenEvent::Delta { .. }) | Ok(TokenEvent::Done { .. })
            ) {
                got_tokens = true;
            }
        }
    }
    let hits_after = mock_hits.load(Ordering::SeqCst);
    let deny_events: Vec<AuditEvent> = bus
        .call(
            "audit.query",
            &AuditQueryRequest {
                trace_id: None,
                actor: Some("service:modeld".into()),
                action: Some("policy.deny".into()),
                last: 50,
            },
            vec![],
        )
        .await
        .unwrap_or_default();
    let secret_denied = deny_events.iter().any(|e| {
        e.detail["rule"] == serde_json::json!("deny_remote_secret")
            && e.target == serde_json::json!("remote:mock:gpt-x")
    });
    gates.push(Gate {
        name: "donnée secret routée local malgré backend distant",
        passed: got_tokens && hits_after == hits_before && secret_denied,
        detail: format!(
            "tokens locaux={got_tokens}, hits mock {}→{}, deny audité={secret_denied}",
            hits_before, hits_after
        ),
    });

    // --- 2. Mode local_only : aucun egress ---
    let _ = bus
        .call::<SetRoutingRequest, Result<(), String>>(
            "model.set_routing",
            &SetRoutingRequest {
                mode: "local_only".into(),
            },
            vec![],
        )
        .await;
    let blocked = bus
        .call_stream::<InferRequest, TokenEvent>(
            "model.infer",
            &InferRequest {
                model_id: Some("remote:mock:gpt-x".into()),
                messages: vec![ChatMessage {
                    role: "user".into(),
                    content: "test".into(),
                }],
                params: InferParams::default(),
                priority: 1,
                data_refs: vec![],
                images: vec![],
                routing: None,
            },
            vec![],
        )
        .await;
    let mut refused = false;
    if let Ok(mut rx) = blocked {
        while let Some(ev) = rx.recv().await {
            if let Ok(TokenEvent::Error { message }) = ev {
                if message.contains("local_only") {
                    refused = true;
                }
            }
        }
    }
    let egress: Vec<EgressEntry> = bus
        .call("net.egress_log", &(), vec![])
        .await
        .unwrap_or_default();
    let mock_egress = egress
        .iter()
        .filter(|e| e.host == "127.0.0.1" && e.allowed)
        .count();
    let hits_final = mock_hits.load(Ordering::SeqCst);
    gates.push(Gate {
        name: "mode local_only : aucun paquet vers le backend distant",
        passed: refused && mock_egress == 0 && hits_final == hits_before,
        detail: format!(
            "refus={refused}, egress autorisés vers mock={mock_egress}, hits mock={hits_final}"
        ),
    });
    let _ = bus
        .call::<SetRoutingRequest, Result<(), String>>(
            "model.set_routing",
            &SetRoutingRequest {
                mode: "balanced".into(),
            },
            vec![],
        )
        .await;

    // --- 3. fs.delete → confirmation bloquante, timeout → refus audité ---
    let del_path = "/documents/notes/gate-delete.md";
    let _ = bus
        .call::<FsWriteRequest, u64>(
            "fs.write",
            &FsWriteRequest {
                path: del_path.into(),
                content: "à supprimer".into(),
                tx_id: None,
                actor: "agent:gate".into(),
                caps: vec!["fs.write:/documents/**".into()],
                trace_id: String::new(),
            },
            vec![],
        )
        .await;
    let t0 = std::time::Instant::now();
    let del = bus
        .call::<FsDeleteRequest, u64>(
            "fs.delete",
            &FsDeleteRequest {
                path: del_path.into(),
                tx_id: None,
                actor: "agent:gate".into(),
                caps: vec!["fs.write:/documents/**".into()],
                trace_id: "gate-p3-delete".into(),
            },
            vec![],
        )
        .await;
    let elapsed = t0.elapsed().as_secs_f64();
    let denied = del.is_err();
    // Le fichier doit exister encore (refus → pas de suppression).
    let still_there = bus
        .call::<FsReadRequest, FsReadResponse>(
            "fs.read",
            &FsReadRequest {
                path: del_path.into(),
                actor: "human:ui".into(),
                caps: vec!["fs.read:/documents/**".into()],
            },
            vec![],
        )
        .await
        .is_ok();
    let resolved: Vec<AuditEvent> = bus
        .call(
            "audit.query",
            &AuditQueryRequest {
                trace_id: Some("gate-p3-delete".into()),
                actor: None,
                action: Some("confirmation.resolved".into()),
                last: 10,
            },
            vec![],
        )
        .await
        .unwrap_or_default();
    let timeout_denied = resolved
        .iter()
        .any(|e| e.detail["approved"] == serde_json::json!(false));
    gates.push(Gate {
        name: "fs.delete : confirmation bloquante, timeout → refus audité",
        passed: denied && still_there && timeout_denied && elapsed >= 2.5,
        detail: format!(
            "refus={denied} après {elapsed:.1}s, fichier présent={still_there}, refus audité={timeout_denied}"
        ),
    });

    // --- 4. Trust : élevé → cap sans confirmation ; faible → refus ---
    let _ = bus
        .call::<TrustSetRequest, bool>(
            "trust.set",
            &TrustSetRequest {
                agent_id: "agent-high".into(),
                score: 0.9,
            },
            vec![],
        )
        .await;
    let high = bus
        .call::<CapRequestRequest, CapRequestOutcome>(
            "cap.request",
            &CapRequestRequest {
                agent_id: "agent-high".into(),
                cap: "fs.write:/documents/**".into(),
                reason: "gate".into(),
            },
            vec![],
        )
        .await;
    let high_granted = matches!(high, Ok(CapRequestOutcome::Granted));
    let _ = bus
        .call::<TrustSetRequest, bool>(
            "trust.set",
            &TrustSetRequest {
                agent_id: "agent-low".into(),
                score: 0.1,
            },
            vec![],
        )
        .await;
    let low = bus
        .call::<CapRequestRequest, CapRequestOutcome>(
            "cap.request",
            &CapRequestRequest {
                agent_id: "agent-low".into(),
                cap: "fs.write:/documents/**".into(),
                reason: "gate".into(),
            },
            vec![],
        )
        .await;
    let low_denied = matches!(low, Ok(CapRequestOutcome::Denied { .. }));
    gates.push(Gate {
        name: "trust élevé → cap accordée sans confirmation ; faible → refus",
        passed: high_granted && low_denied,
        detail: format!("high={high:?}, low={low:?}"),
    });

    println!();
    let mut failed = 0;
    for g in &gates {
        println!(
            "  {} {} — {}",
            if g.passed { "✓" } else { "✗" },
            g.name,
            g.detail
        );
        if !g.passed {
            failed += 1;
        }
    }
    println!(
        "\n=== Gate P3 : {} / {} critères ===",
        gates.len() - failed,
        gates.len()
    );
    if failed > 0 {
        std::process::exit(1);
    }
}
