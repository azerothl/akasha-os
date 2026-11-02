//! Preview troubleshooting report collection.

use crate::cmd::Evt;
use crate::os_open::aos_home;
use crate::security_ui_state::{diagnostic_error_detail, ChatErrorRecord};
use aos_ipc::BusClient;
use aos_proto::{
    AgentInfo, AgentState, FeedbackSubmitRequest, FeedbackSubmitResponse, HealthSnapshot, ModelInfo,
};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::{
    collections::HashSet,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

const RECENT_WINDOW_MS: u64 = 30 * 60 * 1000;
const LOG_TAIL_BYTES: u64 = 64 * 1024;

fn is_recent(at_ms: u64, now_ms: u64) -> bool {
    now_ms
        .checked_sub(at_ms)
        .is_some_and(|age| age <= RECENT_WINDOW_MS)
}

fn modified_ms(path: &Path) -> Option<u64> {
    std::fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis()
        .try_into()
        .ok()
}

fn bounded_log_tail(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let start = file.metadata()?.len().saturating_sub(LOG_TAIL_BYTES);
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::new();
    file.take(LOG_TAIL_BYTES).read_to_end(&mut bytes)?;
    let raw = String::from_utf8_lossy(&bytes);
    let raw = if start > 0 {
        raw.split_once('\n').map(|(_, tail)| tail).unwrap_or("")
    } else {
        &raw
    };
    let mut lines: Vec<_> = raw
        .lines()
        .rev()
        .take(40)
        .map(|line| line.chars().take(1024).collect::<String>())
        .collect();
    lines.reverse();
    Ok(lines.join("\n"))
}

#[derive(Default)]
struct LogSummary {
    text: String,
    errors: usize,
    historical: usize,
    unreadable: usize,
}

fn summarize_logs(run_dir: &Path, now_ms: u64) -> std::io::Result<LogSummary> {
    let mut report = LogSummary {
        text: "## Journaux modifiés dans les 30 dernières minutes\nLa date indique la dernière écriture du fichier, pas celle de chaque ligne. Lecture bornée : 32 fichiers, 40 lignes par fichier (64 Kio maximum).\n".into(),
        ..Default::default()
    };
    let mut files: Vec<_> = std::fs::read_dir(run_dir)?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter(|entry| entry.file_name().to_string_lossy().ends_with(".log"))
        .collect();
    files.sort_by_key(|entry| {
        (
            std::cmp::Reverse(modified_ms(&entry.path())),
            entry.file_name(),
        )
    });
    let omitted = files.len().saturating_sub(32);
    for entry in files.into_iter().take(32) {
        if report.text.len() >= 128 * 1024 {
            report
                .text
                .push_str("Autres journaux omis : limite de taille du rapport atteinte.\n");
            break;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let at = modified_ms(&entry.path());
        let date = at
            .map(|ts| crate::ui_format::format_local_datetime(ts, 0))
            .unwrap_or_else(|| "date inconnue".into());
        if !at.is_some_and(|ts| is_recent(ts, now_ms)) {
            report.historical += 1;
            report.text.push_str(&format!(
                "- {name} : historique ({date} UTC), exclu du décompte actuel.\n"
            ));
            continue;
        }
        match bounded_log_tail(&entry.path()) {
            Ok(tail) if !tail.trim().is_empty() => {
                report.errors += tail
                    .lines()
                    .filter(|line| {
                        let lower = line.to_lowercase();
                        ["error", "panic", "fatal", "échec", "failed"]
                            .iter()
                            .any(|word| lower.contains(word))
                    })
                    .count();
                report.text.push_str(&format!(
                    "### {name} — modifié {date} UTC\n```\n{tail}\n```\n"
                ));
            }
            Err(error) => {
                report.unreadable += 1;
                report
                    .text
                    .push_str(&format!("- {name} : lecture impossible ({error})\n"));
            }
            _ => {}
        }
    }
    if omitted > 0 {
        report.text.push_str(&format!("{omitted} journal(aux) supplémentaire(s) omis, les plus récents ont été examinés en priorité.\n"));
    }
    Ok(report)
}

fn agent_saved_ms(home: &Path, agent_id: &str) -> Option<u64> {
    if agent_id.is_empty()
        || agent_id.len() > 128
        || !agent_id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_".contains(&c))
    {
        return None;
    }
    modified_ms(&home.join("var/agents").join(agent_id).join("info.json"))
}

fn active_agent(agent: &AgentInfo) -> bool {
    !agent.is_roster()
        && matches!(
            agent.state,
            AgentState::Created | AgentState::Running | AgentState::Paused | AgentState::Blocked
        )
}

fn summarize_agents(agents: &[AgentInfo], home: &Path, now_ms: u64) -> (String, Vec<String>) {
    let active = agents.iter().filter(|agent| active_agent(agent)).count();
    let failed = agents
        .iter()
        .filter(|agent| agent.state == AgentState::Failed)
        .count();
    let mut records: Vec<_> = agents
        .iter()
        .map(|agent| (agent, agent_saved_ms(home, &agent.agent_id)))
        .collect();
    records.sort_by_key(|(agent, at)| {
        (
            std::cmp::Reverse(active_agent(agent)),
            std::cmp::Reverse(*at),
            agent.agent_id.clone(),
        )
    });
    let mut findings = Vec::new();
    for (agent, at) in &records {
        if agent.state == AgentState::Failed && at.is_some_and(|ts| is_recent(ts, now_ms)) {
            let reason = agent
                .fail_reason
                .as_deref()
                .and_then(diagnostic_error_detail)
                .unwrap_or_else(|| "motif absent ou expurgé".into());
            findings.push(format!(
                "agent {} en échec récent : {reason}",
                agent.agent_id
            ));
        }
    }
    let mut text = format!("- agents actifs : {active}\n- agents enregistrés : {} (dont {failed} en échec terminal)\n- échecs enregistrés dans les 30 dernières minutes : {}\n", agents.len(), findings.len());
    text.push_str("  Les états terminaux anciens ou sans date vérifiable restent un historique, sans diagnostic de panne actuelle. Dates : dernière sauvegarde locale.\n");
    for (agent, at) in records.into_iter().take(12) {
        let date = at
            .map(|ts| crate::ui_format::format_local_datetime(ts, 0))
            .unwrap_or_else(|| "date inconnue".into());
        text.push_str(&format!(
            "  - {} [{:?}] step {}/{} — {date} UTC\n",
            agent.agent_id, agent.state, agent.step, agent.max_steps
        ));
    }
    (text, findings)
}

fn summarize_chat_errors(errors: &[ChatErrorRecord], now_ms: u64) -> (String, Vec<String>) {
    let mut recent: Vec<_> = errors
        .iter()
        .filter(|error| is_recent(error.at_ms, now_ms))
        .collect();
    recent.sort_by_key(|error| std::cmp::Reverse(error.at_ms));
    let mut seen = HashSet::new();
    let mut text = String::new();
    let mut findings = Vec::new();
    for error in &recent {
        let key = (
            &error.session_id,
            &error.code,
            &error.cause,
            &error.technical_detail,
        );
        if !seen.insert(key) {
            continue;
        }
        if findings.len() == 10 {
            break;
        }
        let count = recent
            .iter()
            .filter(|other| {
                (
                    &other.session_id,
                    &other.code,
                    &other.cause,
                    &other.technical_detail,
                ) == key
            })
            .count();
        let session = error.session_id.as_deref().unwrap_or("sans session");
        let date = crate::ui_format::format_local_datetime(error.at_ms, 0);
        text.push_str(&format!(
            "- {date} UTC — session {session} — {} : {} ({count} occurrence(s))\n",
            error.code, error.cause
        ));
        if let Some(detail) = &error.technical_detail {
            text.push_str(&format!("  Détail technique : {detail}\n"));
        }
        findings.push(format!(
            "erreur chat {} (session {session}, {count} occurrence(s)) : {}",
            error.code, error.cause
        ));
    }
    if !text.is_empty() {
        text.insert_str(0, "## Erreurs chat (30 dernières minutes)\n");
    }
    (text, findings)
}

/// Collecte un diagnostic Preview, l'archive localement et préremplit l'onglet Retour.
pub(crate) async fn run_troubleshoot(
    bus: &Arc<BusClient>,
    evt_tx: &Sender<Evt>,
    recent_chat_errors: Vec<ChatErrorRecord>,
) {
    let _ = evt_tx.send(Evt::Status("Dépannage : collecte des diagnostics…".into()));
    let home = aos_home();
    let now_ms = crate::ui_format::now_ms();
    let version = std::fs::read_to_string(home.join("VERSION"))
        .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").into())
        .trim()
        .to_string();

    let mut findings: Vec<String> = Vec::new();
    let mut sections: Vec<String> = Vec::new();

    sections.push(format!(
        "## Environnement\n- version: {version}\n- AOS_HOME: {}\n- os: {}",
        home.display(),
        std::env::consts::OS
    ));

    // NVIDIA
    let nvidia = std::process::Command::new("nvidia-smi")
        .args(["-L"])
        .output();
    match nvidia {
        Ok(o) if o.status.success() => {
            let text = String::from_utf8_lossy(&o.stdout).trim().to_string();
            sections.push(format!("## NVIDIA\n```\n{text}\n```"));
            if text.is_empty() {
                findings.push("nvidia-smi -L OK mais sortie vide".into());
            }
        }
        Ok(o) => {
            let err = String::from_utf8_lossy(&o.stderr);
            findings.push(format!("nvidia-smi a échoué : {err}"));
            sections.push(format!("## NVIDIA\nERREUR:\n```\n{err}\n```"));
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            sections.push("## NVIDIA\nnvidia-smi absent : information matérielle indisponible (CPU ou autre GPU possible).".into());
        }
        Err(e) => {
            findings.push(format!("nvidia-smi introuvable : {e}"));
            sections.push(format!("## NVIDIA\nintrouvable: {e}"));
        }
    }

    // Logs daemons (dernières lignes)
    let run_dir = home.join("var/run");
    match summarize_logs(&run_dir, now_ms) {
        Ok(report) => {
            if report.errors > 0 {
                findings.push(format!(
                    "{} ligne(s) d'erreur dans les journaux modifiés dans les 30 dernières minutes",
                    report.errors
                ));
            }
            if report.unreadable > 0 {
                findings.push(format!(
                    "{} journal(aux) récent(s) illisible(s)",
                    report.unreadable
                ));
            }
            sections.push(report.text);
        }
        Err(error) => {
            sections.push(format!("## Logs daemons\nvar/run inaccessible : {error}"));
            findings.push("var/run inaccessible".into());
        }
    }

    // Services via bus
    let mut svc = String::from("## Services (bus)\n");
    match bus
        .call::<(), Vec<AgentInfo>>(aos_agent::intents::LIST, &(), vec![])
        .await
    {
        Ok(agents) => {
            let (text, agent_findings) = summarize_agents(&agents, &home, now_ms);
            svc.push_str(&text);
            findings.extend(agent_findings);
        }
        Err(e) => {
            findings.push(format!("agent.list inaccessible : {e}"));
            svc.push_str(&format!("- agent.list ERREUR : {e}\n"));
        }
    }
    match bus
        .call::<(), Vec<ModelInfo>>("model.list", &(), vec![])
        .await
    {
        Ok(models) => {
            svc.push_str(&format!("- modèles : {}\n", models.len()));
            for m in models.iter().take(8) {
                svc.push_str(&format!("  - {} [{:?}]\n", m.id, m.state));
            }
        }
        Err(e) => {
            findings.push(format!("model.list inaccessible : {e}"));
            svc.push_str(&format!("- model.list ERREUR : {e}\n"));
        }
    }
    match bus
        .call::<(), serde_json::Value>("module.list", &(), vec![])
        .await
    {
        Ok(v) => svc.push_str(&format!("- modules : {v}\n")),
        Err(e) => {
            findings.push(format!("module.list inaccessible : {e}"));
            svc.push_str(&format!("- module.list ERREUR : {e}\n"));
        }
    }
    match bus
        .call::<(), HealthSnapshot>("health.snapshot", &(), vec![])
        .await
    {
        Ok(h) => {
            svc.push_str(&format!(
                "- health canary : {} · steps {} · SLO breaches {} · anomaly {:.2}{}\n",
                if h.canary_ok { "ok" } else { "FAILED" },
                h.steps.len(),
                h.slo.breaches.len(),
                h.anomaly.score,
                if h.anomaly.fitted {
                    ""
                } else {
                    " (not fitted)"
                }
            ));
            if !h.canary_ok {
                findings.push("health canary failed".into());
                for step in h.steps.iter().filter(|s| !s.ok) {
                    findings.push(format!(
                        "canary step {} : {}",
                        step.name,
                        step.error.as_deref().unwrap_or("failed")
                    ));
                }
            }
            for b in &h.slo.breaches {
                findings.push(format!("SLO breach: {b}"));
            }
            let _ = evt_tx.send(Evt::Health(h));
        }
        Err(e) => {
            findings.push(format!("health.snapshot inaccessible : {e}"));
            svc.push_str(&format!("- health.snapshot ERREUR : {e}\n"));
        }
    }
    sections.push(svc);

    let (chat_block, chat_findings) = summarize_chat_errors(&recent_chat_errors, now_ms);
    if !chat_block.is_empty() {
        sections.push(chat_block);
    }
    findings.extend(chat_findings);

    let healthy = findings.is_empty();
    let summary = if healthy {
        "Aucune anomalie évidente détectée. Les logs daemons restent disponibles sous var/run/."
            .to_string()
    } else {
        format!(
            "{} anomalie(s) :\n{}",
            findings.len(),
            findings
                .iter()
                .map(|f| format!("- {f}"))
                .collect::<Vec<_>>()
                .join("\n")
        )
    };

    let body = format!(
        "## Résumé dépannage automatique\n\n{summary}\n\n{}\n",
        sections.join("\n")
    );

    let _ = evt_tx.send(Evt::Status(if healthy {
        "Dépannage : OK — aucune anomalie majeure".into()
    } else {
        format!(
            "Dépannage : {} anomalie(s) — rapport en cours…",
            findings.len()
        )
    }));

    // Archive locale + brouillon dans l'onglet Retour (l'utilisateur publie l'issue).
    let req = FeedbackSubmitRequest {
        title: if healthy {
            format!("[Preview][diag] OK — v{version}")
        } else {
            format!(
                "[Preview][bug] Dépannage auto — {} anomalie(s)",
                findings.len()
            )
        },
        category: if healthy {
            "other".into()
        } else {
            "bug".into()
        },
        severity: if healthy {
            "low".into()
        } else if findings.len() >= 3 {
            "high".into()
        } else {
            "medium".into()
        },
        body,
        attachments: vec![],
        scenario: Some("troubleshooting".into()),
        meta: serde_json::json!({
            "preview_version": version,
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "source": "troubleshooting_button",
            "collected_at_ms": now_ms,
            "recent_window_ms": RECENT_WINDOW_MS,
            "findings": findings,
            "healthy": healthy,
        }),
        // Copie locale seulement : l'issue GitHub est créée quand l'utilisateur
        // envoie le formulaire prérempli, pour que le rapport y figure.
        publish_github: false,
    };

    match bus
        .call::<FeedbackSubmitRequest, FeedbackSubmitResponse>("feedback.submit", &req, vec![])
        .await
    {
        Ok(r) => {
            let _ = evt_tx.send(Evt::FeedbackOk(r));
            let mut draft = req;
            draft.publish_github = !healthy;
            let _ = evt_tx.send(Evt::FeedbackDraft(draft));
            let _ = evt_tx.send(Evt::Status(if healthy {
                "Dépannage OK — rapport local prêt dans l'onglet Retour".into()
            } else {
                format!(
                    "Dépannage : {} anomalie(s) — rapport prêt, envoyez-le depuis Retour",
                    findings.len()
                )
            }));
        }
        Err(e) => {
            let _ = evt_tx.send(Evt::Error(format!(
                "Dépannage : échec feedback.submit : {e}"
            )));
            // Même en cas d'échec de la sauvegarde locale, on pré-remplit le formulaire
            // Retour pour que l'utilisateur puisse quand même remonter l'issue avec le rapport.
            let mut draft = req;
            draft.publish_github = !healthy;
            let _ = evt_tx.send(Evt::FeedbackDraft(draft));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{Duration, UNIX_EPOCH};

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let nonce = std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir()
                .join(format!("aos-troubleshoot-{}-{nonce}", std::process::id()));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn write(&self, name: &str, text: &str, at_ms: u64) {
            let path = self.0.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, text).unwrap();
            std::fs::File::options()
                .write(true)
                .open(path)
                .unwrap()
                .set_times(
                    std::fs::FileTimes::new()
                        .set_modified(UNIX_EPOCH + Duration::from_millis(at_ms)),
                )
                .unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            assert!(self.0.starts_with(std::env::temp_dir()));
            assert!(self
                .0
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("aos-troubleshoot-"));
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn old_and_future_logs_do_not_report_current_failures() {
        let f = Fixture::new();
        let now = 2_000_000_000;
        f.write(
            "old.stderr.log",
            "fatal archived startup",
            now - RECENT_WINDOW_MS - 1,
        );
        f.write("future.stderr.log", "fatal clock skew", now + 10_000);
        f.write("aos-agentd.stderr.log", "service ready", now - 100);
        let report = summarize_logs(&f.0, now).unwrap();
        assert_eq!(report.errors, 0);
        assert_eq!(report.historical, 2);
        assert!(report.text.contains("old.stderr.log : historique"));
        assert!(!report.text.contains("archived startup"));
        assert!(report.text.contains("service ready"));
    }

    #[test]
    fn large_log_is_read_from_its_bounded_tail() {
        let f = Fixture::new();
        let now = 2_000_000_000;
        let text = format!(
            "{}\nfatal current failure\n",
            "older filler\n".repeat(20_000)
        );
        f.write("model.stderr.log", &text, now - 100);
        let report = summarize_logs(&f.0, now).unwrap();
        assert_eq!(report.errors, 1);
        assert!(report.text.contains("fatal current failure"));
        assert!(report.text.len() < LOG_TAIL_BYTES as usize);
    }

    #[test]
    fn current_logs_are_not_hidden_by_the_historical_file_limit() {
        let f = Fixture::new();
        let now = 2_000_000_000;
        for i in 0..33 {
            f.write(
                &format!("old-{i:02}.log"),
                "fatal old failure",
                now - RECENT_WINDOW_MS - 1,
            );
        }
        f.write("zz-current.log", "fatal current failure", now - 100);
        let report = summarize_logs(&f.0, now).unwrap();
        assert_eq!(report.errors, 1);
        assert!(report.text.contains("fatal current failure"));
        assert!(report
            .text
            .contains("2 journal(aux) supplémentaire(s) omis"));
    }

    fn agent(id: &str, state: AgentState) -> AgentInfo {
        serde_json::from_value(serde_json::json!({
            "agent_id":id,"state":state,"directive":"fixture","pid":null,
            "caps":[],"last_output":"","fail_reason":"max_steps (64) atteint"
        }))
        .unwrap()
    }

    #[test]
    fn recent_failure_after_twelve_archived_agents_is_still_reported() {
        let f = Fixture::new();
        let now = 2_000_000_000;
        let mut agents = Vec::new();
        for i in 0..13 {
            let id = format!("agent-{i}");
            f.write(
                &format!("var/agents/{id}/info.json"),
                "{}",
                now - RECENT_WINDOW_MS - 1000,
            );
            agents.push(agent(&id, AgentState::Failed));
        }
        f.write("var/agents/agent-recent/info.json", "{}", now - 100);
        agents.push(agent("agent-recent", AgentState::Failed));
        agents.push(agent("agent-working", AgentState::Running));
        agents.push(agent("agent-roster", AgentState::Roster));
        agents.push(agent("agent-undated", AgentState::Failed));
        let (text, findings) = summarize_agents(&agents, &f.0, now);
        assert!(text.contains("agents actifs : 1"));
        assert_eq!(findings.len(), 1);
        assert!(findings[0].contains("agent-recent"));
        assert!(findings[0].contains("max_steps"));
        assert!(text.contains("agent-recent [Failed]"));
    }

    #[test]
    fn chat_diagnostics_keep_session_time_and_deduplicate_only_matching_errors() {
        let now = 2_000_000_000;
        let record = |at_ms, session: &str| ChatErrorRecord {
            at_ms,
            session_id: Some(session.into()),
            code: "chat.error".into(),
            cause: "Something went wrong".into(),
            technical_detail: Some("provider HTTP 429".into()),
        };
        let records = vec![
            record(now - 1, "s1"),
            record(now - 2, "s1"),
            record(now - 3, "s2"),
            record(now - RECENT_WINDOW_MS - 1, "old"),
            record(now + 1, "future"),
        ];
        let (text, findings) = summarize_chat_errors(&records, now);
        assert_eq!(findings.len(), 2);
        assert!(text.contains("session s1"));
        assert!(text.contains("2 occurrence(s)"));
        assert!(text.contains("UTC"));
        assert!(text.contains("provider HTTP 429"));
        assert!(!text.contains("session old"));
        assert!(!text.contains("session future"));
    }
}
