//! Bounded, isolated agent-driven illustration benchmark. No hand-authored pose.
use aos_ipc::BusClient;
use aos_proto::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() == 3 && args[0] == "--stop" {
        let bus = BusClient::connect(&args[1], "illustration-agent-probe").await?;
        let id: AgentCreateResponse = serde_json::from_slice(&std::fs::read(std::path::Path::new(&args[2]).join("agent.json"))?)?;
        let session: ChatSessionMeta = serde_json::from_slice(&std::fs::read(std::path::Path::new(&args[2]).join("session.json"))?)?;
        let current: AgentInfo = bus.call("agent.state", &AgentIdRequest {agent_id:id.agent_id.clone()}, vec![]).await?;
        if current.session_id.as_deref() != Some(session.id.as_str()) {
            return Err("agent does not belong to this benchmark fixture".into());
        }
        let stopped: bool = bus.call("agent.kill", &AgentIdRequest {agent_id:id.agent_id}, vec![]).await?;
        println!("Benchmark stop: {stopped}");
        return Ok(());
    }
    if args.len() != 4 { return Err("usage: illustration_agent_probe BUS MODEL SUBJECT NEW_OUTPUT_DIR".into()); }
    let output = std::path::Path::new(&args[3]);
    std::fs::create_dir(output)?;
    let bus = BusClient::connect(&args[0], "illustration-agent-probe").await?;
    let session: ChatSessionMeta = bus.call("chat.session.create", &ChatSessionCreateRequest {
        title: Some("Illustration · génération autonome de test".into()), model_id: Some(args[1].clone()),
    }, vec![]).await?;
    let _: ChatSessionMeta = bus.call("illust.set_open", &IllustSetOpenRequest {session_id:session.id.clone(), open:true}, vec![]).await?;
    let mut request = AgentCreateRequest::simple(format!(
        "Crée une illustration pour cette demande utilisateur : {}. Utilise le module Illustration de la session courante. Prépare toi-même la construction adaptée au sujet, lance le moteur image, suis les passes sans relancer un calcul en cours, puis examine le rendu réel et rapporte honnêtement les défauts. N'invente pas de validation artistique. Reste dans ce module, sans sous-agent ni autre fichier utilisateur.", args[2]));
    request.model_id = Some(args[1].clone());
    request.session_id = Some(session.id.clone());
    request.gate_mode = "autonomous".into();
    request.tools = ["illust.set_brief", "illust.generate_image", "illust.get", "illust.refine_image", "illust.resolve_image", "illust.export", "goal.complete"].into_iter().map(String::from).collect();
    request.goal = Some(AgentGoal { statement: request.directive.clone(), success_criteria: vec![
        "Un véritable rendu image correspondant à la demande avec passes publiées".into(),
        "Inspection honnête du rendu, sans confondre succès technique et qualité artistique".into(),
    ], max_steps: 24, max_subagents: 0, timeout_secs: 600 });
    request.budget = AgentBudget {max_steps:Some(24),max_tokens:Some(24000)};
    std::fs::write(output.join("request.json"), serde_json::to_vec_pretty(&request)?)?;
    std::fs::write(output.join("session.json"), serde_json::to_vec_pretty(&session)?)?;
    let created: AgentCreateResponse = bus.call("agent.create", &request, vec![]).await?;
    std::fs::write(output.join("agent.json"), serde_json::to_vec_pretty(&created)?)?;
    println!("Session {} ; agent {}", session.id, created.agent_id);
    let started = std::time::Instant::now();
    let mut last_state = String::new();
    let mut last_doc = String::new();
    let mut publication = 0;
    loop {
        let agent: AgentInfo = bus.call("agent.state", &AgentIdRequest {agent_id:created.agent_id.clone()}, vec![]).await?;
        let state = format!("{:?}:{}", agent.state, agent.step);
        if state != last_state {
            println!("{:.1}s {state}", started.elapsed().as_secs_f32());
            std::fs::write(output.join(format!("agent-step-{}-{:?}.json", agent.step, agent.state)), serde_json::to_vec_pretty(&agent)?)?;
            last_state = state;
        }
        if let Ok(doc) = bus.call::<_, IllustGetResponse>("illust.get", &IllustGetRequest {session_id:session.id.clone()}, vec![]).await {
            let serialized = serde_json::to_string_pretty(&doc)?;
            if serialized != last_doc {
                publication += 1;
                std::fs::write(output.join(format!("illustration-{publication}.json")), &serialized)?;
                last_doc = serialized;
            }
        }
        if matches!(agent.state, AgentState::Done | AgentState::Failed | AgentState::Killed | AgentState::Blocked | AgentState::Paused) {
            println!("Agent terminal/attention: {:?}; {}", agent.state, agent.last_output);
            break;
        }
        if started.elapsed().as_secs() > 650 { return Err("observation deadline; inspect agent.state before continuing".into()); }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    Ok(())
}
