//! Acceptance-only client of the production agentd lifecycle API.
use aos_ipc::BusClient;
use aos_proto::{
    AgentCreateRequest, AgentCreateResponse, AgentIdRequest, AgentInfo, AgentOutputEvent,
    AgentSpec, AgentState, AgentTrace,
};
use serde_json::{json, Value};
use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

type Error = Box<dyn std::error::Error>;

// JSON checkpoint decoding may move a floating-point timing by a few ULPs.
// Keep all transcript text, integers, structure and order exact; tolerate only
// this rounding in telemetry floats, never missing steps or changed results.
fn equivalent(left: &Value, right: &Value) -> bool {
    let (mut left, mut right) = (left.clone(), right.clone());
    if let (Some(a), Some(b)) = (
        left.get_mut("steps").and_then(Value::as_array_mut),
        right.get_mut("steps").and_then(Value::as_array_mut),
    ) {
        if a.len() != b.len() {
            return false;
        }
        for (a, b) in a.iter_mut().zip(b.iter_mut()) {
            for key in ["tok_s", "ttft_ms"] {
                if let (Some(x), Some(y)) = (
                    a.get(key).and_then(Value::as_f64),
                    b.get(key).and_then(Value::as_f64),
                ) {
                    if (x - y).abs() > 8.0 * f64::EPSILON * x.abs().max(y.abs()).max(1.0) {
                        return false;
                    }
                    a[key] = json!(0.0);
                    b[key] = json!(0.0);
                }
            }
        }
    }
    left == right
}

async fn start(
    exe: &Path,
    home: &Path,
    addr: &str,
    gate: &str,
    log: &str,
) -> Result<tokio::process::Child, Error> {
    let log = std::fs::File::create(home.join(log))?;
    Ok(tokio::process::Command::new(exe)
        .arg(addr)
        .current_dir(home)
        .env("AOS_HOME", home)
        .env("AOS_MODEL_GATE", gate)
        .stdout(log.try_clone()?)
        .stderr(log)
        .kill_on_drop(true)
        .spawn()?)
}

async fn ready(bus: &BusClient, child: &mut tokio::process::Child) -> Result<(), Error> {
    for _ in 0..300 {
        if child.try_wait()?.is_some() {
            return Err("agentd exited before registration".into());
        }
        if bus
            .call::<(), bool>("bus.lookup:agent.create", &(), vec![])
            .await
            .unwrap_or(false)
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err("agentd registration timed out".into())
}

/// Create through agentd, wait for its child reaper, then restart only the daemon
/// and compare the persisted task, trace and exact scoped grants through its API.
/// Live subscription is retained separately: the API trace is authoritative for
/// steps because subscribers are not promised replay of startup events.
pub async fn run(
    exe: &Path,
    home: &Path,
    addr: &str,
    spec: &AgentSpec,
    gate: &str,
) -> Result<(Vec<AgentOutputEvent>, Value, String), Error> {
    let bus = BusClient::connect(addr, "lifecycle-acceptance").await?;
    let mut daemon = start(exe, home, addr, gate, "agentd.log").await?;
    ready(&bus, &mut daemon).await?;
    let request: AgentCreateRequest = serde_json::from_value(json!({
        "directive":spec.goal.statement,"goal":spec.goal,"tools":spec.tools,
        "caps":spec.caps,"policy":spec.policy,"model_id":spec.model_id,
        "gate_mode":spec.gate_mode,"system_prompt":spec.system_prompt
    }))?;
    let created = bus
        .call::<_, AgentCreateResponse>("agent.create", &request, vec![])
        .await?;
    let id = AgentIdRequest {
        agent_id: created.agent_id.clone(),
    };
    let live = Arc::new(Mutex::new(Vec::new()));
    let sink = live.clone();
    let subscription = bus
        .call_stream::<_, AgentOutputEvent>(
            "agent.subscribe",
            &aos_agent::SubscribeRequest {
                agent_id: id.agent_id.clone(),
            },
            vec![],
        )
        .await;
    let mut rx = match subscription {
        Ok(rx) => rx,
        Err(error) => {
            let _ = bus.call::<_, bool>("agent.kill", &id, vec![]).await;
            daemon.kill().await?;
            return Err(error.into());
        }
    };
    let collector = tokio::spawn(async move {
        while let Some(Ok(event)) = rx.recv().await {
            sink.lock().unwrap().push(event);
        }
    });
    let result = tokio::time::timeout(
        Duration::from_secs(spec.goal.timeout_secs.saturating_add(30)),
        async {
            loop {
                let info = bus.call::<_, AgentInfo>("agent.state", &id, vec![]).await?;
                if info.pid.is_none()
                    && matches!(
                        info.state,
                        AgentState::Done | AgentState::Failed | AgentState::Killed
                    )
                {
                    return Ok::<_, Error>(info);
                }
                if daemon.try_wait()?.is_some() {
                    return Err("agentd exited during task".into());
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        },
    )
    .await;
    // On failure ask the owner to kill its child before dropping the daemon.
    let info = match result {
        Ok(Ok(info)) => info,
        other => {
            let _ = bus.call::<_, bool>("agent.kill", &id, vec![]).await;
            collector.abort();
            daemon.kill().await?;
            let reason = match other {
                Ok(Err(error)) => error.to_string(),
                Err(error) => error.to_string(),
                Ok(Ok(_)) => unreachable!(),
            };
            return Err(format!("agentd task did not finish: {reason}").into());
        }
    };
    let trace = bus
        .call::<_, AgentTrace>("agent.trace", &id, vec![])
        .await?;
    let disk = home.join("var/agents").join(&id.agent_id);
    let before_spec = std::fs::read(disk.join("spec.json"))?;
    let saved_spec: AgentSpec = serde_json::from_slice(&before_spec)?;
    let registry: Value =
        serde_json::from_slice(&std::fs::read(home.join("var/agents/registry.json"))?)?;
    let registered = registry["agents"]
        .as_array()
        .is_some_and(|a| a.contains(&json!(id.agent_id)));
    let exact_grants = saved_spec.caps == spec.caps && info.caps == spec.caps;
    std::fs::write(
        home.join("agent-live-events.json"),
        serde_json::to_vec_pretty(&*live.lock().unwrap())?,
    )?;
    collector.abort();
    daemon.kill().await?;
    daemon.wait().await?;
    // Wait for the broker to remove the dead service before testing registration.
    for _ in 0..300 {
        if !bus
            .call::<(), bool>("bus.lookup:agent.create", &(), vec![])
            .await
            .unwrap_or(false)
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let mut restarted = start(exe, home, addr, gate, "agentd-restarted.log").await?;
    ready(&bus, &mut restarted).await?;
    let restored = bus.call::<_, AgentInfo>("agent.state", &id, vec![]).await?;
    let restored_trace = bus
        .call::<_, AgentTrace>("agent.trace", &id, vec![])
        .await?;
    let list = bus
        .call::<_, Vec<AgentInfo>>("agent.list", &(), vec![])
        .await?;
    let before_trace = serde_json::to_value(&trace)?;
    let after_trace = serde_json::to_value(&restored_trace)?;
    std::fs::write(
        home.join("trace-before-restart.json"),
        serde_json::to_vec_pretty(&before_trace)?,
    )?;
    std::fs::write(
        home.join("trace-after-restart.json"),
        serde_json::to_vec_pretty(&after_trace)?,
    )?;
    let trace_equal = equivalent(&before_trace, &after_trace);
    let spec_equal = before_spec == std::fs::read(disk.join("spec.json"))?;
    let restored_once = list.iter().filter(|a| a.agent_id == id.agent_id).count() == 1;
    let restored_terminal = restored.state == info.state && restored.pid.is_none();
    let restored_profile = restored.caps == info.caps
        && restored.tools == info.tools
        && restored.model_id == info.model_id
        && restored.last_output == info.last_output
        && restored.tokens_used == info.tokens_used;
    restarted.kill().await?;
    restarted.wait().await?;
    let mut events = live
        .lock()
        .unwrap()
        .iter()
        .filter(|e| {
            !matches!(
                e,
                AgentOutputEvent::Step(_) | AgentOutputEvent::StateChanged { .. }
            )
        })
        .cloned()
        .collect::<Vec<_>>();
    events.extend(trace.steps.iter().cloned().map(AgentOutputEvent::Step));
    events.push(AgentOutputEvent::StateChanged {
        state: info.state.clone(),
    });
    let report = json!({"ok":info.state == AgentState::Done && registered && exact_grants
        && trace_equal && spec_equal && restored_once && restored_terminal && restored_profile,
        "agent_id":id.agent_id,"terminal_info":info,"restored_info":restored,
        "registry_contains_task":registered,"exact_scoped_grants":exact_grants,
        "trace_preserved_after_restart":trace_equal,"spec_preserved_after_restart":spec_equal,
        "trace_comparison":"Exact text, integer values, structure and ordering; floating telemetry tolerance 8 * f64::EPSILON * max(abs(a), abs(b), 1)",
        "restored_exactly_once":restored_once,"terminal_state_preserved":restored_terminal,
        "profile_and_result_preserved":restored_profile,
        "worker_reaped_by_agentd":info.pid.is_none(),"owned_agentd_processes_stopped":true,
        "event_sources":"Steps and final state from agentd API; logs/tokens from live subscription (no startup replay guarantee)"});
    std::fs::write(
        home.join("lifecycle.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    Ok((events, report, id.agent_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn trace_comparison_accepts_only_rounding_not_lost_records_or_content() {
        let source = json!({"steps":[{"step":0,"result":"source","tok_s":64.53218514676723}]});
        let mut rounded = source.clone();
        rounded["steps"][0]["tok_s"] = json!(64.53218514676722);
        assert!(equivalent(&source, &rounded));
        rounded["steps"][0]["tok_s"] = json!(64.5);
        assert!(!equivalent(&source, &rounded));
        rounded = source.clone();
        rounded["steps"][0]["result"] = json!("changed");
        assert!(!equivalent(&source, &rounded));
        rounded = source.clone();
        rounded["steps"] = json!([]);
        assert!(!equivalent(&source, &rounded));
        assert!(!equivalent(
            &json!(9007199254740992u64),
            &json!(9007199254740993u64)
        ));
    }
}
