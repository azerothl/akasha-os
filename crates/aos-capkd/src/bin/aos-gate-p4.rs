//! `aos-gate-p4` — vérification des critères P4 testables sur l'hôte (ADR 0001).
//!
//! Prérequis : services démarrés (`demo/run-demo.ps1 -Gate p4`), dont
//! `aos-capkd` et `aos-auditd`.
//!
//! Critères (plan-développement §P4) :
//! 1. Services essentiels isolés (Model, Agent, Storage, Policy, Audit, CapKernel) ;
//! 2. Une capacité révoquée au noyau est **immédiatement** invalide pour
//!    tous les processus (P4.2) — y compris `aos-platformd` ;
//! 3. Kill d'un service non critique (Audit) sans impact sur Model ni UI ;
//! 4. Boot offline → assistant conversationnel fonctionnel.
//!
//! Le port seL4 réel (drivers GPU) est reporté — ADR 0001.

use aos_ipc::{kernel_cap, BusClient, CallError, Status};
use aos_proto::*;
use std::time::Duration;

struct Gate {
    name: &'static str,
    passed: bool,
    detail: String,
}

fn kill_auditd() {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/IM", "aos-auditd.exe", "/F"])
            .output();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("pkill")
            .args(["-f", "aos-auditd"])
            .output();
    }
}

/// Sonde l'état du service d'audit autonome via un intent qu'il est seul à
/// servir (`auditd.verify`). `audit.query` est servi par platformd (journal
/// local) et ne reflète donc pas l'état d'auditd.
async fn audit_reachable(bus: &BusClient) -> bool {
    bus.call::<(), bool>("auditd.verify", &(), vec![]).await.is_ok()
}

async fn wait_intent(bus: &BusClient, intent: &str) -> bool {
    for _ in 0..30 {
        if bus.lookup(intent).await.unwrap_or(false) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}

async fn infer_ok(bus: &BusClient, prompt: &str, max_tokens: u32) -> bool {
    let Ok(mut rx) = bus
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
                    ..Default::default()
                },
                priority: 3,
                data_refs: vec![],
                routing: None,
            },
            vec![],
        )
        .await
    else {
        return false;
    };
    while let Some(ev) = rx.recv().await {
        if matches!(ev, Ok(TokenEvent::Delta { .. }) | Ok(TokenEvent::Done { .. })) {
            return true;
        }
    }
    false
}

#[tokio::main]
async fn main() {
    let bus_addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| format!("127.0.0.1:{}", aos_ipc::DEFAULT_BUS_PORT));
    let bus = BusClient::connect(&bus_addr, "gate-p4")
        .await
        .expect("bus injoignable — lancer la démo d'abord");
    let mut gates: Vec<Gate> = Vec::new();

    println!("=== Gate P4 — critères testables sur l'hôte (ADR 0001) ===\n");

    // --- 1. Services essentiels isolés (P4.4) ---
    let essential = [
        ("model.list", "Model"),
        ("agent.list", "Agent"),
        ("fs.read", "Storage"),
        ("policy.evaluate", "Policy"),
        ("auditd.verify", "Audit"),
        ("cap.check", "CapKernel"),
    ];
    let mut up = Vec::new();
    let mut down = Vec::new();
    for (intent, name) in essential {
        if wait_intent(&bus, intent).await {
            up.push(name);
        } else {
            down.push(name);
        }
    }
    gates.push(Gate {
        name: "services essentiels isolés (Model, Agent, Storage, Policy, Audit, CapKernel)",
        passed: down.is_empty(),
        detail: if down.is_empty() {
            format!("up: {}", up.join(", "))
        } else {
            format!("manquants: {}", down.join(", "))
        },
    });

    // --- 2. Révocation kernel immédiate, cross-process (P4.2) ---
    let path = "/p4/gate.md";
    let object = format!("fs:{path}");
    let mint = bus
        .call::<CapMintRequest, CapMintResponse>(
            "cap.mint",
            &CapMintRequest {
                holder: "agent:gate".into(),
                object: object.clone(),
                rights: vec!["read".into(), "write".into()],
            },
            vec![],
        )
        .await;
    let cap_id = mint.map(|m| m.cap_id).unwrap_or(u64::MAX);
    let uri = kernel_cap(cap_id);
    let write_ok = bus
        .call::<FsWriteRequest, u64>(
            "fs.write",
            &FsWriteRequest {
                path: path.into(),
                content: "secret-p4".into(),
                tx_id: None,
                actor: "agent:gate".into(),
                caps: vec![],
                trace_id: "gate-p4".into(),
            },
            vec![uri.clone()],
        )
        .await
        .is_ok();
    let read_before = bus
        .call::<FsReadRequest, FsReadResponse>(
            "fs.read",
            &FsReadRequest {
                path: path.into(),
                actor: "agent:gate".into(),
                caps: vec![],
            },
            vec![uri.clone()],
        )
        .await
        .is_ok();
    let _ = bus
        .call::<CapRevokeRequest, u64>(
            "cap.revoke",
            &CapRevokeRequest {
                holder: "agent:gate".into(),
                cap: cap_id,
                tree: false,
            },
            vec![],
        )
        .await;
    // Vérification IMMÉDIATE (aucun délai) : noyau + autre processus.
    let kernel_after = bus
        .call::<CapCheckRequest, CapCheckResponse>(
            "cap.check",
            &CapCheckRequest {
                holder: "agent:gate".into(),
                cap: cap_id,
                rights: vec!["read".into()],
                object: Some(object),
            },
            vec![],
        )
        .await
        .map(|r| r.allowed)
        .unwrap_or(true);
    let read_after = bus
        .call::<FsReadRequest, FsReadResponse>(
            "fs.read",
            &FsReadRequest {
                path: path.into(),
                actor: "agent:gate".into(),
                caps: vec![],
            },
            vec![uri],
        )
        .await;
    let platformd_denied = matches!(
        read_after,
        Err(CallError::Status {
            status: Status::PermissionDenied,
            ..
        })
    );
    gates.push(Gate {
        name: "capacité révoquée au noyau → immédiatement invalide pour tous les processus",
        passed: write_ok && read_before && !kernel_after && platformd_denied,
        detail: format!(
            "write={write_ok}, read_avant={read_before}, cap.check_après={kernel_after}, fs.read_après_denied={platformd_denied} (cap {cap_id})"
        ),
    });

    // --- 3. Isolation de panne : kill Audit, Model Subsystem + UI intacts ---
    let audit_up_before = audit_reachable(&bus).await;
    let model_up_before = bus
        .call::<(), Vec<ModelInfo>>("model.list", &(), vec![])
        .await
        .is_ok();
    kill_auditd();
    tokio::time::sleep(Duration::from_millis(800)).await;
    let audit_up_after = audit_reachable(&bus).await;
    let model_up_after = bus
        .call::<(), Vec<ModelInfo>>("model.list", &(), vec![])
        .await
        .is_ok();
    let infer_after_kill = infer_ok(&bus, "dis ok", 4).await;
    gates.push(Gate {
        name: "kill du service Audit → Model Subsystem et UI intacts",
        passed: audit_up_before
            && !audit_up_after
            && model_up_before
            && model_up_after
            && infer_after_kill,
        detail: format!(
            "audit {audit_up_before}→{audit_up_after}, model.list {model_up_before}→{model_up_after}, inférence={infer_after_kill}"
        ),
    });

    // --- 4. Boot offline → assistant conversationnel ---
    let got = infer_ok(&bus, "bonjour", 8).await;
    gates.push(Gate {
        name: "boot offline → assistant conversationnel fonctionnel",
        passed: got,
        detail: format!("tokens locaux={got}"),
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
        "\n=== Gate P4 : {} / {} critères testables sur l'hôte ===",
        gates.len() - failed,
        gates.len()
    );
    println!("(Port seL4 réel reporté — ADR 0001 : noyau de caps userspace + processus isolés.)");
    if failed > 0 {
        std::process::exit(1);
    }
}
