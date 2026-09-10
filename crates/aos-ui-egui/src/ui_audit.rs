//! Onglet Audit v2 (S7.6) : journal exploitable, pas une simple liste.
//!
//! - Filtres locaux (recherche, problèmes, session, chaîne) + `last` serveur.
//! - Lignes : horodatage, badge acteur, action, cible, détail JSON extensible.
//! - Marqueurs de trous temporels (arrêt possible, heuristique documentée).
//! - Intégrité : `audit.verify` (détection d'altération).
//! - Santé : redémarrages watchdog (`var/run/daemon_restarts.log`, écrit par
//!   `aos-session`) + queues stderr des daemons.

use crate::cmd::Cmd;
use crate::ui_format::{format_local_datetime, local_tz_offset_minutes};
use crate::{i18n, icons, UiApp};
use aos_proto::AuditEvent;
use eframe::egui;

/// Seuil au-delà duquel un trou entre deux événements signale un arrêt possible.
pub const AUDIT_GAP_MS: u64 = 5 * 60 * 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActorFamily {
    Agent,
    Service,
    Human,
    Module,
    Other,
}

pub(crate) fn actor_family(actor: &str) -> ActorFamily {
    let kind = actor.split(':').next().unwrap_or("");
    match kind {
        "agent" => ActorFamily::Agent,
        "service" | "platformd" | "modeld" | "agentd" | "session" | "auditd" => {
            ActorFamily::Service
        }
        "human" => ActorFamily::Human,
        "module" => ActorFamily::Module,
        _ => ActorFamily::Other,
    }
}

/// Vrai pour les refus/erreurs : deny, denied, error, fail, refus, conflict.
pub(crate) fn is_problem_action(action: &str) -> bool {
    let lower = action.to_ascii_lowercase();
    ["deny", "denied", "error", "fail", "refus", "conflict"]
        .iter()
        .any(|k| lower.contains(k))
}

/// "trou de 2h05 — arrêt possible ?" / "12 min gap — stopped?".
pub(crate) fn format_gap_ms(gap_ms: u64, fr: bool) -> String {
    let mins = gap_ms / 60_000;
    let body = if mins >= 60 {
        format!("{}h{:02}", mins / 60, mins % 60)
    } else {
        format!("{mins} min")
    };
    if fr {
        format!("trou de {body} — arrêt possible ?")
    } else {
        format!("{body} gap — stopped?")
    }
}

#[derive(Debug, Clone)]
pub(crate) struct DaemonRestart {
    pub(crate) ms: u64,
    pub(crate) daemon: String,
    pub(crate) ok: bool,
}

/// Parse `<ms> <daemon> restarted|restart-failed` (une ligne).
pub(crate) fn parse_restart_line(line: &str) -> Option<DaemonRestart> {
    let mut parts = line.split_whitespace();
    let ms: u64 = parts.next()?.parse().ok()?;
    let daemon = parts.next()?.to_string();
    match parts.next()? {
        "restarted" => Some(DaemonRestart {
            ms,
            daemon,
            ok: true,
        }),
        "restart-failed" => Some(DaemonRestart {
            ms,
            daemon,
            ok: false,
        }),
        _ => None,
    }
}

fn read_restarts(home: &std::path::Path) -> Vec<DaemonRestart> {
    let raw = std::fs::read_to_string(home.join("var/run/daemon_restarts.log")).unwrap_or_default();
    let mut out: Vec<DaemonRestart> = raw.lines().filter_map(parse_restart_line).collect();
    out.sort_by_key(|r| r.ms);
    out
}

fn read_stderr_tail(home: &std::path::Path, daemon: &str, max_lines: usize) -> Vec<String> {
    let raw = std::fs::read_to_string(home.join(format!("var/run/{daemon}.stderr.log")))
        .unwrap_or_default();
    raw.lines()
        .map(str::to_string)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .take(max_lines)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

impl UiApp {
    pub(crate) fn refresh_audit(&mut self) {
        let _ = self.cmd_tx.send(Cmd::Audit {
            last: self.security_ui.audit_last_or_default(),
            actor: None,
            action: None,
            trace_id: self.security_ui.audit_trace.clone(),
        });
    }

    /// Recharge redémarrages + logs stderr (appelée à l'ouverture et au Refresh).
    pub(crate) fn refresh_audit_health(&mut self) {
        let home = crate::os_open::aos_home();
        let mut restarts = read_restarts(&home);
        restarts.truncate(200);
        self.security_ui.audit_restarts = restarts;
        let mut logs = Vec::new();
        for daemon in ["aos-platformd", "aos-modeld", "aos-agentd", "aos-auditd"] {
            let tail = read_stderr_tail(&home, daemon, 8);
            if !tail.is_empty() {
                logs.push((daemon.to_string(), tail));
            }
        }
        self.security_ui.audit_logs = logs;
    }

    /// Acteurs `agent:<id>` rattachés à la session (pour le filtre session).
    fn audit_session_agents(&self, session_id: &str) -> Vec<String> {
        self.agents
            .iter()
            .filter(|a| a.session_id.as_deref() == Some(session_id))
            .map(|a| format!("agent:{}", a.agent_id))
            .collect()
    }

    fn audit_visible(&self) -> Vec<AuditEvent> {
        let q = self.security_ui.audit_search.trim().to_lowercase();
        let session_agents: Option<Vec<String>> = self
            .security_ui
            .audit_session
            .as_deref()
            .map(|sid| self.audit_session_agents(sid));
        let mut events: Vec<AuditEvent> = self
            .security_ui
            .audit
            .iter()
            .filter(|e| {
                if self.security_ui.audit_problems_only && !is_problem_action(&e.action) {
                    return false;
                }
                if let Some(agents) = &session_agents {
                    if !agents.iter().any(|a| a == &e.actor) {
                        return false;
                    }
                }
                if let Some(trace) = &self.security_ui.audit_trace {
                    if &e.trace_id != trace {
                        return false;
                    }
                }
                if !q.is_empty() {
                    let hay = format!(
                        "{} {} {} {} {}",
                        e.actor, e.action, e.target, e.trace_id, e.detail
                    )
                    .to_lowercase();
                    if !hay.contains(q.as_str()) {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();
        events.sort_by_key(|e| e.seq);
        events.reverse();
        events
    }

    pub(crate) fn ui_audit(&mut self, ui: &mut egui::Ui) {
        let t = i18n::strings(&self.prefs.language);
        let fr = self.prefs.language == "fr";
        let tz = local_tz_offset_minutes();
        ui.heading(t.audit_heading);
        if self.security_ui.audit.is_empty() {
            ui.weak(if fr {
                "Aucun événement d'audit pour le moment."
            } else {
                "No audit events yet."
            });
        }

        // Intégrité de la chaîne.
        match self.security_ui.audit_verified {
            Some(true) => {
                ui.horizontal(|ui| {
                    icons::done_check(ui);
                    ui.colored_label(
                        crate::theme::button_colors(ui).success,
                        if fr {
                            "Chaîne intègre (audit.verify)"
                        } else {
                            "Chain intact (audit.verify)"
                        },
                    );
                });
            }
            Some(false) => {
                ui.colored_label(
                    crate::theme::button_colors(ui).danger,
                    if fr {
                        "! CHAÎNE ALTÉRÉE — journal à vérifier"
                    } else {
                        "! CHAIN TAMPERED — inspect the journal"
                    },
                );
            }
            None => {}
        }

        ui.horizontal_wrapped(|ui| {
            if ui.button(t.decl_ui_refresh).clicked() {
                self.refresh_audit();
                self.refresh_audit_health();
                self.security_ui.audit_verified = None;
            }
            if ui
                .small_button(if fr { "Vérifier" } else { "Verify" })
                .on_hover_text(if fr {
                    "Vérifie la chaîne de hash (auditd.verify)"
                } else {
                    "Verify the hash chain (auditd.verify)"
                })
                .clicked()
            {
                let _ = self.cmd_tx.send(Cmd::AuditVerify);
            }
            // Nombre d'événements demandés.
            let mut last = self.security_ui.audit_last_or_default();
            egui::ComboBox::from_id_salt("audit_last")
                .selected_text(format!("{last}"))
                .show_ui(ui, |ui| {
                    for n in [50usize, 200, 500] {
                        if ui.selectable_label(last == n, format!("{n}")).clicked() {
                            last = n;
                        }
                    }
                });
            if last != self.security_ui.audit_last_or_default() {
                self.security_ui.audit_last = last;
                self.refresh_audit();
            }
            // Garde-fou destructeur : KillAuditd à côté de Refresh.
            if crate::ui_primitives::danger_confirm_button(
                ui,
                "audit-kill",
                t.audit_kill_p4,
                &format!("{} ?", t.audit_kill_p4),
            ) {
                let _ = self.cmd_tx.send(Cmd::KillAuditd);
            }
        });

        // Filtres.
        ui.horizontal_wrapped(|ui| {
            crate::ui_primitives::search_field(
                ui,
                &mut self.security_ui.audit_search,
                t.settings_search_label,
                if fr {
                    "Acteur, action, cible, trace…"
                } else {
                    "Actor, action, target, trace…"
                },
                t.search_field_clear,
            );
        });
        ui.horizontal_wrapped(|ui| {
            let mut problems = self.security_ui.audit_problems_only;
            if ui
                .checkbox(
                    &mut problems,
                    if fr {
                        "Problèmes uniquement"
                    } else {
                        "Problems only"
                    },
                )
                .changed()
            {
                self.security_ui.audit_problems_only = problems;
            }
            // Sessions.
            let current = self.security_ui.audit_session.clone();
            let mut picked: Option<Option<String>> = None;
            egui::ComboBox::from_id_salt("audit_session")
                .selected_text(
                    current
                        .as_deref()
                        .and_then(|id| {
                            self.chat_state
                                .sessions
                                .iter()
                                .find(|s| s.id == id)
                                .map(|s| s.title.clone())
                        })
                        .unwrap_or_else(|| {
                            if fr {
                                "Toutes sessions".into()
                            } else {
                                "All sessions".into()
                            }
                        }),
                )
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(
                            current.is_none(),
                            if fr {
                                "Toutes sessions"
                            } else {
                                "All sessions"
                            },
                        )
                        .clicked()
                    {
                        picked = Some(None);
                    }
                    for s in self.chat_state.sessions.clone() {
                        if ui
                            .selectable_label(current.as_deref() == Some(s.id.as_str()), &s.title)
                            .clicked()
                        {
                            picked = Some(Some(s.id.clone()));
                        }
                    }
                });
            if let Some(sel) = picked {
                self.security_ui.audit_session = sel;
            }
            // Chaîne ciblée.
            if let Some(trace) = self.security_ui.audit_trace.clone() {
                ui.weak(if fr { "chaîne :" } else { "trace:" });
                ui.monospace(&trace);
                if icons::close_button(ui).clicked() {
                    self.security_ui.audit_trace = None;
                    self.refresh_audit();
                }
            }
        });

        // Santé : redémarrages watchdog + stderr daemons.
        egui::CollapsingHeader::new(if fr {
            "Santé (crashs/redémarrages)"
        } else {
            "Health (crashes/restarts)"
        })
        .default_open(!self.security_ui.audit_restarts.is_empty())
        .show(ui, |ui| {
            if self.security_ui.audit_restarts.is_empty() {
                ui.weak(if fr {
                    "Aucun redémarrage watchdog enregistré."
                } else {
                    "No watchdog restarts recorded."
                });
            } else {
                for r in self
                    .security_ui
                    .audit_restarts
                    .clone()
                    .into_iter()
                    .rev()
                    .take(20)
                {
                    ui.horizontal(|ui| {
                        if r.ok {
                            icons::refresh_mark(ui, crate::theme::button_colors(ui).success);
                        } else {
                            ui.colored_label(crate::theme::button_colors(ui).danger, "!");
                        }
                        ui.monospace(format_local_datetime(r.ms, tz));
                        ui.label(format!(
                            "{} — {}",
                            r.daemon,
                            if r.ok {
                                if fr {
                                    "redémarré"
                                } else {
                                    "restarted"
                                }
                            } else if fr {
                                "redémarrage ÉCHOUÉ"
                            } else {
                                "restart FAILED"
                            }
                        ));
                    });
                }
            }
            if !self.security_ui.audit_logs.is_empty() {
                ui.separator();
            }
            for (daemon, lines) in self.security_ui.audit_logs.clone() {
                egui::CollapsingHeader::new(daemon)
                    .default_open(false)
                    .show(ui, |ui| {
                        for line in lines {
                            ui.monospace(&line);
                        }
                    });
            }
        });
        ui.separator();

        // Liste, la plus récente d'abord, avec marqueurs de trous.
        let events = self.audit_visible();
        if events.is_empty() && !self.security_ui.audit.is_empty() {
            ui.weak(t.settings_search_empty);
        }
        let mut prev_ts: Option<u64> = None;
        for e in &events {
            if let Some(prev) = prev_ts {
                if prev.saturating_sub(e.ts_ms) > AUDIT_GAP_MS {
                    ui.weak(format_gap_ms(prev - e.ts_ms, fr));
                    ui.separator();
                }
            }
            prev_ts = Some(e.ts_ms);
            let selected = self.security_ui.audit_selected == Some(e.seq);
            let row = ui.horizontal(|ui| {
                ui.weak(format_local_datetime(e.ts_ms, tz));
                let (label, color) = match actor_family(&e.actor) {
                    ActorFamily::Agent => ("agent", crate::theme::button_colors(ui).accent),
                    ActorFamily::Human => ("humain", crate::theme::button_colors(ui).success),
                    ActorFamily::Module => ("module", crate::theme::button_colors(ui).warning),
                    ActorFamily::Service => ("service", ui.visuals().weak_text_color()),
                    ActorFamily::Other => ("?", ui.visuals().weak_text_color()),
                };
                ui.colored_label(color, label);
                ui.strong(&e.action);
                let target = crate::agent_panel::truncate(&e.target, 48);
                ui.label(target).on_hover_text(&e.target);
                if is_problem_action(&e.action) {
                    ui.colored_label(crate::theme::button_colors(ui).danger, "!");
                }
            });
            if row.response.clicked() {
                self.security_ui.audit_selected = if selected { None } else { Some(e.seq) };
            }
            if selected {
                ui.group(|ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.weak(format!("#{}", e.seq));
                        ui.monospace(format_local_datetime(e.ts_ms, tz));
                        ui.label(&e.actor);
                    });
                    ui.label(&e.target);
                    if !e.trace_id.is_empty() {
                        ui.horizontal_wrapped(|ui| {
                            ui.weak("trace:");
                            ui.monospace(&e.trace_id);
                            if ui
                                .small_button(if fr { "Chaîne" } else { "Chain" })
                                .clicked()
                            {
                                self.security_ui.audit_trace = Some(e.trace_id.clone());
                                self.refresh_audit();
                            }
                        });
                    }
                    let pretty = serde_json::to_string_pretty(&e.detail)
                        .unwrap_or_else(|_| e.detail.to_string());
                    if pretty != "null" && !pretty.is_empty() {
                        egui::ScrollArea::horizontal()
                            .id_salt(("audit_detail", e.seq))
                            .show(ui, |ui| {
                                ui.monospace(pretty);
                            });
                    }
                    ui.horizontal_wrapped(|ui| {
                        ui.weak(format!(
                            "hash {}… · prev {}…",
                            e.hash.chars().take(8).collect::<String>(),
                            e.prev_hash.chars().take(8).collect::<String>(),
                        ));
                    });
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actor_family_classifies_prefixes() {
        assert_eq!(actor_family("agent:abc"), ActorFamily::Agent);
        assert_eq!(actor_family("human:ui"), ActorFamily::Human);
        assert_eq!(actor_family("module:notes"), ActorFamily::Module);
        assert_eq!(actor_family("service:platformd"), ActorFamily::Service);
        assert_eq!(actor_family("platformd"), ActorFamily::Service);
        assert_eq!(actor_family("???"), ActorFamily::Other);
    }

    #[test]
    fn problems_match_deny_error_fail() {
        assert!(is_problem_action("policy.deny"));
        assert!(is_problem_action("cap.deny"));
        assert!(is_problem_action("device.capture.error"));
        assert!(!is_problem_action("model.migrate.fallback"));
        assert!(!is_problem_action("fs.write"));
        assert!(!is_problem_action("mem.extract"));
    }

    #[test]
    fn gap_formats_hours_and_minutes() {
        assert_eq!(
            format_gap_ms(12 * 60_000, true),
            "trou de 12 min — arrêt possible ?"
        );
        assert_eq!(
            format_gap_ms(125 * 60_000, true),
            "trou de 2h05 — arrêt possible ?"
        );
        assert_eq!(format_gap_ms(7 * 60_000, false), "7 min gap — stopped?");
    }

    #[test]
    fn restarts_parse_ok_and_failed() {
        let ok = parse_restart_line("1725000000000 aos-modeld restarted").unwrap();
        assert_eq!(
            (ok.ms, ok.daemon.as_str(), ok.ok),
            (1725000000000, "aos-modeld", true)
        );
        let ko = parse_restart_line("1725000000001 aos-modeld restart-failed").unwrap();
        assert!(!ko.ok);
        assert!(parse_restart_line("garbage").is_none());
        assert!(parse_restart_line("123 nosuchaction").is_none());
    }
}
