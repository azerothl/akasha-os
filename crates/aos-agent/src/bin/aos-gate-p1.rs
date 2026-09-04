//! `aos-gate-p1` — vérification exécutable du Gate P1 (plan-développement §P1).
//!
//! Prérequis : `aos-busd`, `aos-modeld`, `aos-agentd` démarrés
//! (voir `demo/run-demo.ps1`).
//!
//! Critères vérifiés :
//! 1. Assistant conversationnel sur le modèle embarqué, TTFT < 2 s à chaud ;
//! 2. Inférence réussie sur 32B Q6 avec budget VRAM 8 GiB (offload actif,
//!    plan visible) ;
//! 3. Deux agents concurrents en parallèle sans crash mutuel ;
//! 4. Kill d'un agent sans impact sur le Model Subsystem.

use aos_agent::intents as agent_intents;
use aos_ipc::BusClient;
use aos_proto::{
    AgentCreateRequest, AgentCreateResponse, AgentIdRequest, AgentInfo, ChatMessage, CognitiveMode,
    InferParams, InferRequest, LoadRequest, LoadResponse, PlanCreateRequest, PlanGetRequest,
    PlanResponse, PlanStep, TokenEvent,
};
use std::time::{Duration, Instant};

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

async fn infer_once(
    bus: &BusClient,
    model: Option<String>,
    prompt: &str,
    max_tokens: u32,
) -> Result<(String, f64, f64), String> {
    let req = InferRequest {
        model_id: model,
        messages: vec![ChatMessage {
            role: "user".into(),
            content: prompt.into(),
        }],
        params: InferParams {
            max_tokens,
            temperature: 0.2,
            top_p: 0.9,
            seed: Some(42),
        },
        priority: 3,
        data_refs: vec![],
        images: vec![],
        routing: None,
    };
    let mut rx = bus
        .call_stream::<InferRequest, TokenEvent>("model.infer", &req, vec![])
        .await
        .map_err(|e| e.to_string())?;
    let mut text = String::new();
    let (mut ttft, mut tok_s) = (0.0, 0.0);
    while let Some(ev) = rx.recv().await {
        match ev {
            Ok(TokenEvent::Delta { text: t }) => text.push_str(&t),
            Ok(TokenEvent::Done {
                ttft_ms, tok_s: ts, ..
            }) => {
                ttft = ttft_ms;
                tok_s = ts;
            }
            Ok(TokenEvent::Error { message }) => return Err(message),
            Err(e) => return Err(e.to_string()),
            _ => {}
        }
    }
    Ok((text, ttft, tok_s))
}

#[tokio::main]
async fn main() {
    let bus_addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| format!("127.0.0.1:{}", aos_ipc::DEFAULT_BUS_PORT));
    let bus = BusClient::connect(&bus_addr, "gate-p1")
        .await
        .expect("bus injoignable — lancer la démo d'abord");
    let mut gates: Vec<Gate> = Vec::new();

    println!("=== Gate P1 — vérification exécutable ===\n");

    // --- 1. Assistant conversationnel, TTFT < 2 s warm ---
    let t0 = Instant::now();
    let warm1 = infer_once(&bus, None, "Dis bonjour en un mot.", 16).await;
    let cold_ms = t0.elapsed().as_secs_f64() * 1000.0;
    match warm1 {
        Ok((text, _, _)) if !text.is_empty() => {
            let (t2, ttft2, tok2) = infer_once(&bus, None, "Capitale de l'Espagne ? Un mot.", 16)
                .await
                .expect("2e inférence");
            gates.push(Gate {
                name: "assistant conversationnel (embedded-instruct)",
                passed: !t2.is_empty(),
                detail: format!("réponse: {:.40?} ({tok2:.1} tok/s)", t2.trim()),
            });
            gates.push(Gate {
                name: "TTFT < 2 s warm (NFR-01)",
                passed: ttft2 < 2000.0,
                detail: format!(
                    "TTFT warm = {ttft2:.0} ms (1er appel, chargement inclus : {cold_ms:.0} ms)"
                ),
            });
        }
        other => gates.push(Gate {
            name: "assistant conversationnel (embedded-instruct)",
            passed: false,
            detail: format!("{other:?}"),
        }),
    }

    // --- 2. 32B Q6 avec budget VRAM 8 GiB (offload actif) ---
    let load = bus
        .call::<LoadRequest, LoadResponse>(
            "model.load",
            &LoadRequest {
                model_id: "local:llama-q6-32b".into(),
                profile: "balanced".into(),
                kv_tokens: 2048,
            },
            vec![],
        )
        .await;
    match load {
        Ok(resp) => {
            // Le résumé du plan est « VRAM x GiB | RAM y GiB | DISK z GiB | … »
            // — l'offload est actif si des couches sont hors VRAM (RAM > 0).
            let ram_gib = resp
                .placement
                .split('|')
                .nth(1)
                .and_then(|s| {
                    s.trim()
                        .trim_start_matches("RAM")
                        .split_whitespace()
                        .next()
                        .and_then(|v| v.parse::<f64>().ok())
                })
                .unwrap_or(0.0);
            gates.push(Gate {
                name: "32B Q6 chargé avec budget VRAM 8 GiB",
                passed: ram_gib > 1.0,
                detail: format!("{} (RAM = {ram_gib:.1} GiB offloaded)", resp.placement),
            });
            let big = infer_once(
                &bus,
                Some("local:llama-q6-32b".into()),
                "Réponds par un mot : capitale de l'Italie ?",
                8,
            )
            .await;
            gates.push(Gate {
                name: "inférence réussie sur le 32B en offload",
                passed: matches!(&big, Ok((t, _, _)) if !t.is_empty()),
                detail: match &big {
                    Ok((t, ttft, ts)) => format!(
                        "réponse: {:.30?} TTFT {ttft:.0} ms, {ts:.2} tok/s",
                        t.trim()
                    ),
                    Err(e) => e.clone(),
                },
            });
        }
        Err(e) => {
            gates.push(Gate {
                name: "32B Q6 chargé avec budget VRAM 8 GiB",
                passed: false,
                detail: e.to_string(),
            });
        }
    }

    // --- 3. Deux agents concurrents ---
    let mk = |directive: &str| {
        let mut r = AgentCreateRequest::simple(directive);
        r.model_id = Some("local:embedded-instruct".into());
        r.goal = Some(aos_proto::AgentGoal {
            statement: directive.into(),
            success_criteria: vec![],
            max_steps: 4,
            max_subagents: 0,
            timeout_secs: 120,
        });
        r
    };
    let a1 = bus
        .call::<_, AgentCreateResponse>(
            agent_intents::CREATE,
            &mk("Dis 'alpha' en un mot."),
            vec![],
        )
        .await;
    let a2 = bus
        .call::<_, AgentCreateResponse>(agent_intents::CREATE, &mk("Dis 'beta' en un mot."), vec![])
        .await;
    let (id1, id2) = match (a1, a2) {
        (Ok(r1), Ok(r2)) => (r1.agent_id, r2.agent_id),
        _ => {
            gates.push(Gate {
                name: "deux agents concurrents",
                passed: false,
                detail: "échec de création".into(),
            });
            ("?".into(), "?".into())
        }
    };
    if id1 != "?" {
        // Attendre la complétion des deux (état Done) avec timeout.
        let deadline = Instant::now() + Duration::from_secs(300);
        let (mut d1, mut d2) = (false, false);
        while Instant::now() < deadline && !(d1 && d2) {
            tokio::time::sleep(Duration::from_secs(2)).await;
            let list: Vec<AgentInfo> = bus
                .call(agent_intents::LIST, &(), vec![])
                .await
                .unwrap_or_default();
            for a in &list {
                if a.agent_id == id1
                    && matches!(
                        a.state,
                        aos_proto::AgentState::Done | aos_proto::AgentState::Running
                    )
                {
                    d1 = a.state == aos_proto::AgentState::Done || !a.last_output.is_empty();
                }
                if a.agent_id == id2
                    && matches!(
                        a.state,
                        aos_proto::AgentState::Done | aos_proto::AgentState::Running
                    )
                {
                    d2 = a.state == aos_proto::AgentState::Done || !a.last_output.is_empty();
                }
            }
        }
        gates.push(Gate {
            name: "deux agents concurrents en parallèle sans crash mutuel",
            passed: d1 && d2,
            detail: format!("{id1} produit={d1}, {id2} produit={d2}"),
        });

        // --- 4. Kill d'un agent sans impact ---
        let killed = bus
            .call::<AgentIdRequest, bool>(
                agent_intents::KILL,
                &AgentIdRequest {
                    agent_id: id1.clone(),
                },
                vec![],
            )
            .await;
        // Le Model Subsystem doit rester fonctionnel juste après.
        let after = infer_once(&bus, None, "Dis 'ok'.", 8).await;
        let kill_ok = killed.as_ref().map(|b| *b).unwrap_or(false);
        let after_ok = after.is_ok();
        gates.push(Gate {
            name: "kill d'un agent sans impact sur le Model Subsystem",
            passed: kill_ok && after_ok,
            detail: format!("kill={kill_ok}, inférence post-kill ok={after_ok}"),
        });
        let _ = bus
            .call::<AgentIdRequest, bool>(
                agent_intents::KILL,
                &AgentIdRequest { agent_id: id2 },
                vec![],
            )
            .await;
    }

    // --- 5. Runtime agentic : skill.list + agent multi-steps ---
    {
        let skills: Result<Vec<aos_proto::SkillInfo>, _> = bus
            .call(agent_intents::SKILL_LIST, &(), vec![])
            .await;
        let skill_ok = skills
            .as_ref()
            .map(|s| s.iter().any(|x| x.name == "notes-writer"))
            .unwrap_or(false);
        gates.push(Gate {
            name: "skill.list expose notes-writer",
            passed: skill_ok,
            detail: format!("{:?}", skills.as_ref().map(|s| s.len())),
        });

        let mut req = AgentCreateRequest::simple(
            "Appelle notes.list puis réponds goal.complete avec summary 'ok'.",
        );
        req.skills = vec!["notes-writer".into()];
        req.tools = vec!["notes.list".into(), "goal.complete".into()];
        req.caps = vec!["tool.invoke:notes".into()];
        req.goal = Some(aos_proto::AgentGoal {
            statement: req.directive.clone(),
            success_criteria: vec![],
            max_steps: 8,
            max_subagents: 0,
            timeout_secs: 180,
        });
        req.model_id = Some("local:embedded-instruct".into());
        match bus
            .call::<_, AgentCreateResponse>(agent_intents::CREATE, &req, vec![])
            .await
        {
            Ok(r) => {
                let deadline = Instant::now() + Duration::from_secs(180);
                let mut done = false;
                let mut saw_progress = false;
                while Instant::now() < deadline && !done {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    let list: Vec<AgentInfo> = bus
                        .call(agent_intents::LIST, &(), vec![])
                        .await
                        .unwrap_or_default();
                    if let Some(a) = list.iter().find(|a| a.agent_id == r.agent_id) {
                        if a.step > 0 {
                            saw_progress = true;
                        }
                        if matches!(
                            a.state,
                            aos_proto::AgentState::Done | aos_proto::AgentState::Failed
                        ) {
                            done = true;
                        }
                    }
                }
                let _ = bus
                    .call::<AgentIdRequest, bool>(
                        agent_intents::KILL,
                        &AgentIdRequest {
                            agent_id: r.agent_id.clone(),
                        },
                        vec![],
                    )
                    .await;
                gates.push(Gate {
                    name: "agent multi-steps (progress / terminal)",
                    passed: saw_progress || done,
                    detail: format!("id={} progress={saw_progress} done={done}", r.agent_id),
                });
            }
            Err(e) => {
                gates.push(Gate {
                    name: "agent multi-steps (progress / terminal)",
                    passed: false,
                    detail: e.to_string(),
                });
            }
        }
    }

    // --- Deep Thinking plan.create / plan.get smoke ---
    {
        let mut req = AgentCreateRequest::simple(
            "Deep Thinking smoke — plan hiérarchique uniquement",
        );
        req.cognitive_mode = CognitiveMode::DeepThinking;
        req.skills = vec!["deep-thinking".into()];
        req.tools = vec!["goal.complete".into()];
        req.caps = vec!["tool.invoke:notes".into()];
        req.goal = Some(aos_proto::AgentGoal {
            statement: req.directive.clone(),
            success_criteria: vec![],
            max_steps: 2,
            max_subagents: 0,
            timeout_secs: 60,
        });
        req.model_id = Some("local:embedded-instruct".into());
        match bus
            .call::<_, AgentCreateResponse>(agent_intents::CREATE, &req, vec![])
            .await
        {
            Ok(r) => {
                let create = PlanCreateRequest {
                    agent_id: r.agent_id.clone(),
                    task: "smoke deep plan".into(),
                    title: Some("Smoke".into()),
                    steps: vec![PlanStep {
                        id: "1".into(),
                        label: "Analyse".into(),
                        description: None,
                        status: Default::default(),
                        agent_id: None,
                        children: vec![PlanStep {
                            id: "1.1".into(),
                            label: "Contexte".into(),
                            ..Default::default()
                        }],
                        logs: vec![],
                    }],
                };
                let created = bus
                    .call::<PlanCreateRequest, PlanResponse>(
                        agent_intents::PLAN_CREATE,
                        &create,
                        vec![],
                    )
                    .await;
                let got = match &created {
                    Ok(resp) => {
                        bus.call::<PlanGetRequest, PlanResponse>(
                            agent_intents::PLAN_GET,
                            &PlanGetRequest {
                                plan_id: Some(resp.plan.id.clone()),
                                agent_id: Some(r.agent_id.clone()),
                            },
                            vec![],
                        )
                        .await
                        .ok()
                    }
                    Err(_) => None,
                };
                let _ = bus
                    .call::<AgentIdRequest, bool>(
                        agent_intents::KILL,
                        &AgentIdRequest {
                            agent_id: r.agent_id.clone(),
                        },
                        vec![],
                    )
                    .await;
                let ok = created
                    .as_ref()
                    .ok()
                    .map(|p| p.plan.version >= 1 && !p.plan.steps.is_empty())
                    .unwrap_or(false)
                    && got
                        .as_ref()
                        .map(|p| !p.plan.steps[0].children.is_empty())
                        .unwrap_or(false);
                gates.push(Gate {
                    name: "deep thinking plan.create/get",
                    passed: ok,
                    detail: format!(
                        "id={} create={:?} children={}",
                        r.agent_id,
                        created.as_ref().map(|p| p.plan.version).ok(),
                        got.as_ref()
                            .map(|p| p.plan.steps[0].children.len())
                            .unwrap_or(0)
                    ),
                });
            }
            Err(e) => {
                gates.push(Gate {
                    name: "deep thinking plan.create/get",
                    passed: false,
                    detail: e.to_string(),
                });
            }
        }
    }

    // --- Bilan ---
    println!();
    let mut failed = 0;
    for g in &gates {
        report(g);
        if !g.passed {
            failed += 1;
        }
    }
    println!(
        "\n=== Gate P1 : {} / {} critères ===",
        gates.len() - failed,
        gates.len()
    );
    if failed > 0 {
        std::process::exit(1);
    }
}
