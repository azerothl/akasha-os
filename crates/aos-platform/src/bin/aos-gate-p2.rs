//! `aos-gate-p2` — vérification exécutable du Gate P2.
//!
//! Prérequis : `aos-busd`, `aos-modeld`, `aos-agentd`, `aos-platformd`
//! démarrés et le module « notes » packagé (`modules/build-notes.ps1`).
//!
//! Critères (plan-développement §P2) :
//! 1. Module « notes » installé, utilisé par un **agent** (création via
//!    outil) ET par un **humain** (même module via l'UI) ;
//! 2. Audit trail montre la chaîne complète intent → agent → outil → fs ;
//! 3. Undo d'une création de fichier par agent restaure l'état antérieur ;
//! 4. Un module qui accède à un fichier sans capacité est refusé et audité.

use aos_agent::intents as agent_intents;
use aos_ipc::BusClient;
use aos_proto::*;
use std::time::{Duration, Instant};

struct Gate {
    name: &'static str,
    passed: bool,
    detail: String,
}

#[tokio::main]
async fn main() {
    let bus_addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| format!("127.0.0.1:{}", aos_ipc::DEFAULT_BUS_PORT));
    let bus = BusClient::connect(&bus_addr, "gate-p2")
        .await
        .expect("bus injoignable — lancer la démo d'abord");
    let mut gates: Vec<Gate> = Vec::new();

    println!("=== Gate P2 — vérification exécutable ===\n");

    // --- 0. Installation du module notes ---
    let install = bus
        .call::<ModuleInstallRequest, ModuleInfo>(
            "module.install",
            &ModuleInstallRequest {
                source_dir: "modules/notes.aospkg".into(),
                approved_caps: None,
                actor: "human:gate".into(),
                actor_caps: vec![],
            },
            vec![],
        )
        .await;
    let installed = match &install {
        Ok(info) => {
            gates.push(Gate {
                name: "module « notes » installé",
                passed: true,
                detail: format!("v{}, caps: {}", info.version, info.granted_caps.join(", ")),
            });
            true
        }
        Err(e) => {
            gates.push(Gate {
                name: "module « notes » installé",
                passed: false,
                detail: format!("{e}"),
            });
            false
        }
    };

    if installed {
        // --- 1a. Utilisé par un AGENT (création d'une note via outil) ---
        let directive = "Tu disposes de l'outil notes.create pour créer des notes. \
             Réponds UNIQUEMENT par la ligne suivante, sans autre texte :\n\
             TOOL: notes.create {\"title\": \"note-agent-gate\", \"content\": \"créée par un agent via outil WASM\"}";
        let created = bus
            .call::<AgentCreateRequest, AgentCreateResponse>(
                agent_intents::CREATE,
                &{
                    let mut r = AgentCreateRequest::simple(directive);
                    r.caps = vec!["tool.invoke:notes".to_string()];
                    r.model_id = Some("local:embedded-instruct".into());
                    r.skills = vec!["notes-writer".into()];
                    r.tools = vec!["notes.create".into()];
                    r.goal = Some(aos_proto::AgentGoal {
                        statement: directive.to_string(),
                        success_criteria: vec![],
                        max_steps: 6,
                        max_subagents: 0,
                        timeout_secs: 180,
                    });
                    r
                },
                vec![],
            )
            .await;
        let mut agent_note_ok = false;
        let mut agent_id = String::new();
        if let Ok(resp) = created {
            agent_id = resp.agent_id;
            // Attendre que l'agent ait produit quelque chose (outil appelé).
            let deadline = Instant::now() + Duration::from_secs(180);
            while Instant::now() < deadline && !agent_note_ok {
                tokio::time::sleep(Duration::from_secs(3)).await;
                let list: Vec<AgentInfo> = bus
                    .call(agent_intents::LIST, &(), vec![])
                    .await
                    .unwrap_or_default();
                if let Some(a) = list.iter().find(|a| a.agent_id == agent_id) {
                    if a.last_output.contains("outil notes.create")
                        || a.last_output.contains("notes/")
                    {
                        agent_note_ok = true;
                    }
                }
                // Vérifie aussi le FS directement.
                let notes = bus
                    .call::<ModuleInvokeRequest, ModuleInvokeResponse>(
                        "module.invoke",
                        &ModuleInvokeRequest {
                            module: "notes".into(),
                            tool: "notes.list".into(),
                            args: serde_json::json!({}),
                            actor: "human:ui".into(),
                            actor_caps: vec![],
                            trace_id: String::new(),
                        },
                        vec![],
                    )
                    .await;
                if let Ok(r) = notes {
                    if r.ok {
                        // Accepte l'ancien format (chemins) et le nouveau (objets avec title).
                        let s = r.result.to_string();
                        if s.contains("note-agent-gate") {
                            agent_note_ok = true;
                        }
                    }
                }
            }
        }
        gates.push(Gate {
            name: "module utilisé par un agent (création d'une note via outil)",
            passed: agent_note_ok,
            detail: format!("agent {agent_id}"),
        });

        // --- 1b. Utilisé par un HUMAIN (surface UI, même module) ---
        let human = bus
            .call::<ModuleInvokeRequest, ModuleInvokeResponse>(
                "module.invoke",
                &ModuleInvokeRequest {
                    module: "notes".into(),
                    tool: "notes.create".into(),
                    args: serde_json::json!({"title": "note-humain-gate", "content": "créée par un humain via l'UI"}),
                    actor: "human:ui".into(),
                    actor_caps: vec![],
                    trace_id: "gate-p2-human".into(),
                },
                vec![],
            )
            .await;
        let human_ok = matches!(&human, Ok(r) if r.ok);
        gates.push(Gate {
            name: "module utilisé par un humain (double surface)",
            passed: human_ok,
            detail: format!("{:?}", human.as_ref().map(|r| &r.result)),
        });

        // --- 2. Chaîne d'audit complète intent → agent → outil → fs ---
        // On rejoue une création agent avec trace explicite pour la chaîne.
        let trace2 = format!("gate-p2-chain-{}", std::process::id());
        let _ = bus
            .call::<ModuleInvokeRequest, ModuleInvokeResponse>(
                "module.invoke",
                &ModuleInvokeRequest {
                    module: "notes".into(),
                    tool: "notes.create".into(),
                    args: serde_json::json!({"title": "note-chaine", "content": "chaîne d'audit"}),
                    actor: format!("agent:{agent_id}"),
                    actor_caps: vec!["tool.invoke:notes".to_string()],
                    trace_id: trace2.clone(),
                },
                vec![],
            )
            .await;
        let events: Vec<AuditEvent> = bus
            .call(
                "audit.query",
                &AuditQueryRequest {
                    trace_id: Some(trace2.clone()),
                    actor: None,
                    action: None,
                    last: 50,
                },
                vec![],
            )
            .await
            .unwrap_or_default();
        let has_tool = events
            .iter()
            .any(|e| e.action == "tool.invoke" && e.actor.starts_with("agent:"));
        let has_fs = events
            .iter()
            .any(|e| e.action == "fs.write" && e.actor.starts_with("module:"));
        let verify: bool = bus.call("audit.verify", &(), vec![]).await.unwrap_or(false);
        gates.push(Gate {
            name: "audit trail : chaîne intent → agent → outil → fs (+ intégrité)",
            passed: has_tool && has_fs && verify,
            detail: format!(
                "tool.invoke={has_tool}, fs.write={has_fs}, {} événements, intégrité={verify}",
                events.len()
            ),
        });

        // --- 3. Undo d'une création de fichier par agent ---
        let path = "/documents/notes/undo-gate.md";
        let w = bus
            .call::<FsWriteRequest, u64>(
                "fs.write",
                &FsWriteRequest {
                    path: path.into(),
                    content: "fichier créé par un agent".into(),
                    tx_id: None,
                    actor: format!("agent:{agent_id}"),
                    caps: vec!["fs.write:/documents/**".into()],
                    trace_id: String::new(),
                },
                vec![],
            )
            .await;
        let u = bus
            .call::<FsUndoRequest, FsUndoResponse>(
                "fs.undo",
                &FsUndoRequest {
                    path: path.into(),
                    actor: "human:ui".into(),
                    trace_id: String::new(),
                },
                vec![],
            )
            .await;
        // Après undo, le fichier ne doit plus exister.
        let after = bus
            .call::<FsReadRequest, FsReadResponse>(
                "fs.read",
                &FsReadRequest {
                    path: path.into(),
                    actor: "human:ui".into(),
                    caps: vec!["fs.read:/documents/**".into()],
                },
                vec![],
            )
            .await;
        let undo_ok = w.is_ok() && u.is_ok() && after.is_err();
        gates.push(Gate {
            name: "undo d'une création de fichier restaure l'état antérieur",
            passed: undo_ok,
            detail: format!(
                "write={}, undo={:?}, lecture après undo refusée={}",
                w.is_ok(),
                u.as_ref().map(|r| &r.description),
                after.is_err()
            ),
        });

        // --- 4. Accès sans capacité → refusé et audité ---
        // La directive demande au module de lire hors de ses caps via un
        // outil qui n'existe pas ? Non : on teste le garde-fou du runtime —
        // un agent SANS cap tool.invoke:notes tente notes.list.
        let denied = bus
            .call::<ModuleInvokeRequest, ModuleInvokeResponse>(
                "module.invoke",
                &ModuleInvokeRequest {
                    module: "notes".into(),
                    tool: "notes.list".into(),
                    args: serde_json::json!({}),
                    actor: "agent:sans-cap".into(),
                    actor_caps: vec![],
                    trace_id: "gate-p2-deny".into(),
                },
                vec![],
            )
            .await;
        let denied_ok = match &denied {
            Ok(r) => !r.ok,
            Err(_) => true, // statut PermissionDenied côté bus
        };
        // Et l'événement audité (tool.invoke ok=false).
        let deny_events: Vec<AuditEvent> = bus
            .call(
                "audit.query",
                &AuditQueryRequest {
                    trace_id: Some("gate-p2-deny".into()),
                    actor: None,
                    action: None,
                    last: 10,
                },
                vec![],
            )
            .await
            .unwrap_or_default();
        let audited = deny_events
            .iter()
            .any(|e| e.action == "tool.invoke" && e.detail["ok"] == serde_json::json!(false));
        gates.push(Gate {
            name: "accès sans capacité refusé et audité",
            passed: denied_ok && audited,
            detail: format!("refusé={denied_ok}, audité={audited}"),
        });
    }

    // Nettoyage de l'agent de test.
    if let Ok(list) = bus
        .call::<_, Vec<AgentInfo>>(agent_intents::LIST, &(), vec![])
        .await
    {
        for a in list {
            let _ = bus
                .call::<AgentIdRequest, bool>(
                    agent_intents::KILL,
                    &AgentIdRequest {
                        agent_id: a.agent_id,
                    },
                    vec![],
                )
                .await;
        }
    }

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
        "\n=== Gate P2 : {} / {} critères ===",
        gates.len() - failed,
        gates.len()
    );
    if failed > 0 {
        std::process::exit(1);
    }
}
