//! Heuristics for Preview auto-troubleshooting (#464 — reduce false positives).

use aos_proto::{AgentInfo, AgentState, HealthSnapshot};

/// True when a daemon log line indicates an actionable incident (not benign noise).
pub(crate) fn log_line_is_incident(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    let lower = trimmed.to_ascii_lowercase();

    if log_line_is_benign(&lower) {
        return false;
    }

    if lower.contains("panicked at")
        || (lower.contains("thread '") && lower.contains("panicked"))
        || lower.contains("note: run with `rust_backtrace`")
    {
        return true;
    }
    if lower.contains("connectionrefused")
        || lower.contains("kind: connectionrefused")
        || (lower.contains("10061") && lower.contains("127.0.0.1"))
    {
        return true;
    }
    if lower.contains("addrinuse") || lower.contains("code: 10048") {
        return true;
    }
    if lower.contains("démarrage échoué") || lower.contains("couldn't finish starting") {
        return true;
    }
    if lower.contains("fatal") {
        return true;
    }
    if lower.starts_with("error:") || lower.contains(" error:") {
        return true;
    }
    if lower.contains("failed") && !lower.contains("canary : ok") {
        return true;
    }
    if lower.contains("échec") || lower.contains("échoué") {
        return true;
    }
    false
}

fn log_line_is_benign(lower: &str) -> bool {
    lower.contains("cuda graph id") && lower.contains("reused")
        || lower.contains("ggml_backend_cuda_graph_compute")
        || lower.contains("embeddings désactivés")
        || lower.contains("embeddings disabled")
        || lower.contains("embeddings unavailable")
        || lower.contains("embeddings indisponibles")
        || lower.contains("product rag skip")
        || lower.contains("user library embed skip")
        || lower.contains("print_info:")
        || lower.contains("[modeld] batch")
        || lower.contains("health canary : ok")
        || lower.contains("connecté au bus")
        || lower.contains("connected to bus")
        || lower.contains("prêt")
        || lower.contains(" journal d'audit prêt")
        || lower.contains(" écoute sur ")
}

/// Historical `Failed` agents are normal when core services are healthy (#112, #120).
pub(crate) fn failed_agent_is_incident(agent: &AgentInfo, infra_healthy: bool) -> bool {
    if !matches!(agent.state, AgentState::Failed) {
        return false;
    }
    if !infra_healthy {
        return true;
    }
    false
}

pub(crate) fn infra_is_healthy(health: Option<&HealthSnapshot>) -> bool {
    health.is_some_and(|h| h.canary_ok)
}

/// Detect rapid daemon restart storms in `daemon_restarts.log`.
pub(crate) fn daemon_restart_loop_findings(raw: &str, now_ms: u64) -> Vec<String> {
    let window_ms = 120_000;
    let mut recent: Vec<&str> = Vec::new();
    for line in raw.lines() {
        let Some((ts, rest)) = line.split_once(' ') else {
            continue;
        };
        let Ok(ts) = ts.parse::<u64>() else {
            continue;
        };
        if now_ms.saturating_sub(ts) > window_ms {
            continue;
        }
        if rest.contains("restarted") {
            recent.push(rest);
        }
    }
    if recent.len() >= 6 {
        vec![format!(
            "daemon restart loop ({}) in the last {}s",
            recent.len(),
            window_ms / 1000
        )]
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aos_proto::HealthAnomaly;

    #[test]
    fn benign_cuda_and_embedding_lines_are_not_incidents() {
        assert!(!log_line_is_incident(
            "CUDA Graph id 1131 reused"
        ));
        assert!(!log_line_is_incident(
            "[aos-platformd] user library embed skip: embeddings désactivés dans ce binaire"
        ));
    }

    #[test]
    fn bus_panic_and_connection_refused_are_incidents() {
        assert!(log_line_is_incident(
            "thread 'main' panicked at crates\\aos-agent\\src\\bin\\aos-agentd.rs:883:10:"
        ));
        assert!(log_line_is_incident(
            "connexion au bus: Io(Os { code: 10061, kind: ConnectionRefused, message: \"…\" })"
        ));
    }

    #[test]
    fn failed_agents_skipped_when_infra_healthy() {
        let agent = AgentInfo {
            agent_id: "agent-59".into(),
            state: AgentState::Failed,
            directive: String::new(),
            pid: None,
            caps: vec![],
            last_output: String::new(),
            step: 64,
            max_steps: 64,
            current_task: None,
            parent_id: None,
            children: vec![],
            tokens_used: 0,
            skills: vec![],
            tools: vec![],
            mcp_servers: vec![],
            fail_reason: None,
            session_id: None,
            model_id: None,
            title: String::new(),
            kind: aos_proto::AgentKind::default(),
            display_name: None,
            persona_id: None,
            source_roster_id: None,
            origin: None,
            avatar: None,
            color: None,
            deep_plan: None,
            cognitive_mode: aos_proto::CognitiveMode::default(),
            execution_backend: aos_proto::AgentExecutionBackend::default(),
        };
        let health = HealthSnapshot {
            canary_ok: true,
            anomaly: HealthAnomaly::default(),
            ..Default::default()
        };
        assert!(!failed_agent_is_incident(&agent, infra_is_healthy(Some(&health))));
        assert!(failed_agent_is_incident(
            &agent,
            infra_is_healthy(Some(&HealthSnapshot {
                canary_ok: false,
                ..Default::default()
            }))
        ));
    }

    #[test]
    fn restart_loop_detected_in_window() {
        let now = 1_000_000_000u64;
        let raw = (0..8)
            .map(|i| format!("{} aos-modeld restarted", now - i * 1000))
            .collect::<Vec<_>>()
            .join("\n");
        let findings = daemon_restart_loop_findings(&raw, now);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].contains("restart loop"));
    }
}
