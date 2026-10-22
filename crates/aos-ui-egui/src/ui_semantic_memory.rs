//! Inspectable semantic concepts and explicit profile review.
use crate::{cmd::Cmd, overflow_scroll_h, UiApp};
use eframe::egui;

fn semantic_kind_label(kind: &aos_proto::semantic_memory::SemanticKind, fr: bool) -> &'static str {
    use aos_proto::semantic_memory::SemanticKind;
    match kind {
        SemanticKind::Concept => "Concept",
        SemanticKind::Preference => {
            if fr {
                "Préférence"
            } else {
                "Preference"
            }
        }
        SemanticKind::Belief => {
            if fr {
                "Conviction"
            } else {
                "Belief"
            }
        }
        SemanticKind::Pattern => {
            if fr {
                "Tendance"
            } else {
                "Pattern"
            }
        }
        SemanticKind::Goal => {
            if fr {
                "Objectif"
            } else {
                "Goal"
            }
        }
        SemanticKind::CommunicationStyle => {
            if fr {
                "Style de communication"
            } else {
                "Communication style"
            }
        }
        SemanticKind::SuccessfulWorkflow => {
            if fr {
                "Méthode réussie"
            } else {
                "Successful workflow"
            }
        }
        SemanticKind::RecurringFailure => {
            if fr {
                "Échec récurrent"
            } else {
                "Recurring failure"
            }
        }
        SemanticKind::PreferredTool => {
            if fr {
                "Outil préféré"
            } else {
                "Preferred tool"
            }
        }
    }
}

impl UiApp {
    pub(crate) fn ui_semantic_memory(&mut self, ui: &mut egui::Ui) {
        let fr = self.prefs.language.starts_with("fr");
        let mut ns = self.memory_ui.semantic_namespace.trim().to_string();
        let mut request = None;
        egui::CollapsingHeader::new(if fr { "Concepts et profils appris" } else { "Learned concepts and profiles" })
            .id_salt("semantic_memory").show(ui, |ui| {
            ui.weak(if fr {
                "Interprétations issues de plusieurs observations. Seuls les concepts confirmés entrent dans le contexte des agents."
            } else {
                "Interpretations derived from multiple observations. Only confirmed concepts enter agent context."
            });
            ui.collapsing(if fr { "Avancé : autre espace mémoire" } else { "Advanced: another memory namespace" }, |ui| {
                ui.text_edit_singleline(&mut self.memory_ui.semantic_namespace);
            });
            ui.horizontal(|ui| {
                ui.label(if fr { "Profil" } else { "Profile" });
                let selected = if self.memory_ui.semantic_namespace == "user:default" {
                    if fr { "Mon profil" } else { "My profile" }
                } else {
                    self.agents.iter().find(|a| format!("agent:{}",a.agent_id) == self.memory_ui.semantic_namespace)
                        .map(|a| a.display_name.as_deref().unwrap_or(&a.title)).unwrap_or(if fr { "Autre profil" } else { "Another profile" })
                };
                egui::ComboBox::from_id_salt("semantic_subject").selected_text(selected).show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.memory_ui.semantic_namespace, "user:default".into(), if fr { "Mon profil" } else { "My profile" });
                    for agent in &self.agents {
                        ui.selectable_value(&mut self.memory_ui.semantic_namespace, format!("agent:{}",agent.agent_id), agent.display_name.as_deref().unwrap_or(&agent.title));
                    }
                });
                ns = self.memory_ui.semantic_namespace.trim().to_string();
                ui.add_enabled_ui(!self.memory_ui.semantic_busy, |ui| {
                    if ui.button(if fr { "Consulter" } else { "Inspect" }).clicked() {
                        request = Some(("mem.semantic.list", serde_json::json!({"namespace":ns})));
                    }
                    if ui.button(if fr { "Consolider" } else { "Consolidate" }).clicked() {
                        request = Some(("mem.semantic.consolidate", serde_json::json!({"namespace":ns})));
                    }
                });
                if self.memory_ui.semantic_busy { ui.spinner(); }
            });
            if let Some(response) = self.memory_ui.semantic.as_ref().filter(|r| r.namespace == ns) {
                let mut enabled = response.enabled;
                if ui.add_enabled(!self.memory_ui.semantic_busy, egui::Checkbox::new(&mut enabled,
                    if fr { "Consolider automatiquement en arrière-plan (modèle local)" } else { "Consolidate in the background (local model)" }
                )).changed() {
                    request = Some(("mem.semantic.settings", serde_json::json!({"namespace":ns,"enabled":enabled})));
                }
                ui.weak(if fr {
                    format!("{} observations traitées · {} en attente", response.processed_sources, response.pending_sources)
                } else {
                    format!("{} observations processed · {} pending", response.processed_sources, response.pending_sources)
                });
                ui.weak(format!("{}: {}", if fr { "Observations exclues (type, secret, statut ou taille)" } else { "Excluded observations (type, secret, status or length)" }, response.excluded_sources));
                overflow_scroll_h(ui, "semantic_concepts", 280.0, |ui| {
                for c in &response.concepts {
                    let status = match c.status {
                        aos_proto::MemoryObjectStatus::Candidate => if fr { "À confirmer" } else { "Needs review" },
                        aos_proto::MemoryObjectStatus::Accepted => if fr { "Confirmé" } else { "Confirmed" },
                        aos_proto::MemoryObjectStatus::Rejected => if fr { "Rejeté" } else { "Rejected" },
                        aos_proto::MemoryObjectStatus::Superseded => if fr { "Remplacé" } else { "Superseded" },
                        aos_proto::MemoryObjectStatus::Archived => if fr { "Archivé" } else { "Archived" },
                    };
                    egui::CollapsingHeader::new(format!("{} · {} · {:.0}%", c.summary, status, c.confidence * 100.0))
                        .id_salt(("semantic_concept", &ns, c.id)).show(ui, |ui| {
                        ui.weak(format!("{} · {} {} · {} source(s)", semantic_kind_label(&c.kind,fr), if fr { "révision" } else { "revision" }, c.revision, c.source_count));
                        ui.weak(if fr { "Score de preuves : indicateur heuristique, pas une probabilité mesurée." } else { "Evidence score: a heuristic, not a measured probability." });
                        if c.disputed { ui.colored_label(ui.visuals().warn_fg_color, if fr { "Observations contradictoires : à réexaminer." } else { "Conflicting observations: needs review." }); }
                        for (label, evidence) in [
                            (if fr { "Sources favorables" } else { "Supporting sources" }, &c.supporting),
                            (if fr { "Sources contradictoires" } else { "Contradicting sources" }, &c.contradicting),
                        ] {
                            if evidence.is_empty() { continue; }
                            ui.strong(label);
                            for source in evidence {
                                ui.label(format!("#{} — {}", source.memory_id, source.excerpt));
                                if ui.small_button(format!("{} #{}", if fr { "Voir le souvenir" } else { "Inspect memory" }, source.memory_id)).clicked() {
                                    self.memory_ui.palace_namespace = ns.clone();
                                    let _ = self.cmd_tx.send(Cmd::MemMindPalace { namespace: Some(ns.clone()), project: None, root_id: Some(source.memory_id) });
                                }
                                for origin in &source.source_refs {
                                    ui.weak(format!("{}: {}", origin.source_type, origin.source_id));
                                    if origin.source_type == "chat_session" && ui.small_button(if fr { "Ouvrir la conversation" } else { "Open conversation" }).clicked() {
                                        let _ = self.cmd_tx.send(Cmd::SessionSelect { id: origin.source_id.clone() });
                                    }
                                    if let Some(uri) = &origin.uri { ui.label(uri); }
                                }
                            }
                        }
                        ui.horizontal(|ui| {
                            if ui.add_enabled(!c.disputed && !self.memory_ui.semantic_busy, egui::Button::new(if fr { "Confirmer" } else { "Confirm" })).clicked() {
                                request = Some(("mem.semantic.review", serde_json::json!({"namespace":ns,"id":c.id,"revision":c.revision,"accepted":true})));
                            }
                            if ui.add_enabled(!self.memory_ui.semantic_busy, egui::Button::new(if fr { "Rejeter" } else { "Reject" })).clicked() {
                                request = Some(("mem.semantic.review", serde_json::json!({"namespace":ns,"id":c.id,"revision":c.revision,"accepted":false})));
                            }
                        });
                        for revision in &c.history {
                            ui.weak(format!("r{} · {} · +{:?} / -{:?}", revision.revision, revision.summary, revision.supporting_ids, revision.contradicting_ids));
                            ui.horizontal_wrapped(|ui| {
                                for id in revision.supporting_ids.iter().chain(&revision.contradicting_ids) {
                                    if ui.small_button(format!("#{id}")).clicked() {
                                        self.memory_ui.palace_namespace = ns.clone();
                                        let _ = self.cmd_tx.send(Cmd::MemMindPalace { namespace: Some(ns.clone()), project: None, root_id: Some(*id) });
                                    }
                                }
                            });
                        }
                    });
                }
                });
            }
        });
        if let Some((intent, payload)) = request {
            self.memory_ui.semantic_busy = self
                .cmd_tx
                .send(Cmd::MemSemantic { intent, payload })
                .is_ok();
        }
    }
}
