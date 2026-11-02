//! Heuristics for Preview auto-troubleshooting (#464 — reduce false positives).

/// True when a daemon log line indicates an actionable incident (not benign noise).
pub(crate) fn log_line_is_incident(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    let lower = trimmed.to_ascii_lowercase();

    // A benign startup/status marker elsewhere on the line must not hide an
    // actual native decode error, panic or fatal failure.
    if lower.contains("inconsistent sequence positions")
        || (lower.contains("llama_decode")
            && (lower.contains("failed") || lower.contains("ret = -") || lower.contains("code -")))
        || lower.contains("panicked")
        || lower.contains("fatal")
    {
        return true;
    }

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
        if !now_ms.checked_sub(ts).is_some_and(|age| age <= window_ms) {
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

    #[test]
    fn benign_cuda_and_embedding_lines_are_not_incidents() {
        assert!(!log_line_is_incident("CUDA Graph id 1131 reused"));
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
    fn native_failures_are_not_hidden_by_status_markers() {
        assert!(log_line_is_incident(
            "[modeld] llama_decode failed: ret = -1"
        ));
        assert!(log_line_is_incident(
            "tokens have inconsistent sequence positions"
        ));
        assert!(log_line_is_incident(
            "ggml_backend_cuda_graph_compute: fatal error"
        ));
    }

    #[test]
    fn future_restart_dates_are_not_current_incidents() {
        let raw = (0..8)
            .map(|i| format!("{} aos-modeld restarted", 100_000 + i))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(daemon_restart_loop_findings(&raw, 99_000).is_empty());
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
