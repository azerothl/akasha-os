//! Preview troubleshooting report collection.

use crate::cmd::Evt;
use crate::os_open::aos_home;
use aos_ipc::BusClient;
use aos_proto::{AgentInfo, AgentState, FeedbackSubmitRequest, FeedbackSubmitResponse, ModelInfo};
use std::sync::mpsc::Sender;
use std::sync::Arc;

/// Collecte un diagnostic Preview, l'archive localement et préremplit l'onglet Retour.
pub(crate) async fn run_troubleshoot(bus: &Arc<BusClient>, evt_tx: &Sender<Evt>) {
    let _ = evt_tx.send(Evt::Status("Dépannage : collecte des diagnostics…".into()));
    let home = aos_home();
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
        Err(e) => {
            findings.push(format!("nvidia-smi introuvable : {e}"));
            sections.push(format!("## NVIDIA\nintrouvable: {e}"));
        }
    }

    // Logs daemons (dernières lignes)
    let run_dir = home.join("var/run");
    let mut log_block = String::from("## Logs daemons (var/run)\n");
    let mut log_errors = 0usize;
    if let Ok(rd) = std::fs::read_dir(&run_dir) {
        let mut files: Vec<_> = rd.filter_map(|e| e.ok()).collect();
        files.sort_by_key(|e| e.file_name());
        for ent in files {
            let name = ent.file_name().to_string_lossy().to_string();
            if !(name.ends_with(".stderr.log")
                || name.ends_with(".stdout.log")
                || name.ends_with(".log"))
            {
                continue;
            }
            let path = ent.path();
            let raw = std::fs::read_to_string(&path).unwrap_or_default();
            let lines: Vec<&str> = raw.lines().collect();
            let start = lines.len().saturating_sub(40);
            let tail = lines[start..].join("\n");
            for line in &lines[start..] {
                let lower = line.to_lowercase();
                if lower.contains("error")
                    || lower.contains("panic")
                    || lower.contains("fatal")
                    || lower.contains("échec")
                    || lower.contains("failed")
                {
                    log_errors += 1;
                }
            }
            if !tail.trim().is_empty() {
                log_block.push_str(&format!("### {name}\n```\n{tail}\n```\n"));
            }
        }
    } else {
        log_block.push_str("(dossier var/run absent)\n");
        findings.push("var/run inaccessible".into());
    }
    if log_errors > 0 {
        findings.push(format!(
            "{log_errors} ligne(s) d'erreur détectée(s) dans les logs récents"
        ));
    }
    sections.push(log_block);

    // Services via bus
    let mut svc = String::from("## Services (bus)\n");
    match bus
        .call::<(), Vec<AgentInfo>>(aos_agent::intents::LIST, &(), vec![])
        .await
    {
        Ok(agents) => {
            svc.push_str(&format!("- agents actifs : {}\n", agents.len()));
            for a in agents.iter().take(12) {
                svc.push_str(&format!(
                    "  - {} [{:?}] step {}/{}\n",
                    a.agent_id, a.state, a.step, a.max_steps
                ));
                if matches!(a.state, AgentState::Failed) {
                    findings.push(format!("agent {} en Failed", a.agent_id));
                }
            }
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
    sections.push(svc);

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
