//! Onglet Notes — liste, éditeur markdown, aperçu, liens, joindre à un agent.

use eframe::egui::{self, RichText, Ui};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteListItem {
    pub title: String,
    pub path: String,
    pub slug: String,
    #[serde(default)]
    pub excerpt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NoteLink {
    pub title: String,
    pub slug: String,
    pub path: String,
    #[serde(default)]
    pub exists: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteDetail {
    pub title: String,
    pub path: String,
    pub slug: String,
    pub content: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub outgoing: Vec<NoteLink>,
    #[serde(default)]
    pub incoming: Vec<NoteLink>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteSearchHit {
    pub id: u64,
    pub text: String,
    pub score: f32,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub slug: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteRelatedHit {
    pub title: String,
    pub path: String,
    pub slug: String,
    pub relation: String,
    pub hops: u32,
    pub score: f32,
    #[serde(default)]
    pub excerpt: String,
}

/// Actions demandées par le panneau (consommées par `UiApp`).
#[derive(Debug, Default)]
pub struct NotesActions {
    pub list: bool,
    pub search: Option<String>,
    pub read_path: Option<String>,
    pub read_title: Option<String>,
    pub save_create: Option<(String, String)>,
    pub save_update: Option<(String, String, String)>, // title, path, body
    pub attach_path: Option<String>,
    pub related: Option<(String, String)>, // path, topic
}

pub struct NotesPanelState {
    pub notes: Vec<NoteListItem>,
    pub filter: String,
    pub search_query: String,
    pub search_hits: Vec<NoteSearchHit>,
    pub related_hits: Vec<NoteRelatedHit>,
    pub selected_path: Option<String>,
    pub edit_title: String,
    pub edit_body: String,
    pub edit_path: Option<String>,
    pub edit_slug: Option<String>,
    pub is_new: bool,
    pub dirty: bool,
    pub outgoing: Vec<NoteLink>,
    pub incoming: Vec<NoteLink>,
    pub status: String,
    pub show_preview: bool,
    md_cache: CommonMarkCache,
}

impl Default for NotesPanelState {
    fn default() -> Self {
        Self {
            notes: Vec::new(),
            filter: String::new(),
            search_query: String::new(),
            search_hits: Vec::new(),
            related_hits: Vec::new(),
            selected_path: None,
            edit_title: String::new(),
            edit_body: String::new(),
            edit_path: None,
            edit_slug: None,
            is_new: true,
            dirty: false,
            outgoing: Vec::new(),
            incoming: Vec::new(),
            status: String::new(),
            show_preview: true,
            md_cache: CommonMarkCache::default(),
        }
    }
}

impl NotesPanelState {
    pub fn apply_listed(&mut self, notes: Vec<NoteListItem>) {
        self.notes = notes;
        self.status = format!("{} note(s)", self.notes.len());
    }

    pub fn apply_loaded(&mut self, detail: NoteDetail) {
        self.selected_path = Some(detail.path.clone());
        self.edit_path = Some(detail.path);
        self.edit_slug = Some(detail.slug);
        self.edit_title = detail.title;
        self.edit_body = if detail.body.is_empty() {
            // Contenu brut sans H1 déjà séparé.
            let (_, body) = split_h1(&detail.content);
            body
        } else {
            detail.body
        };
        self.outgoing = detail.outgoing;
        self.incoming = detail.incoming;
        self.is_new = false;
        self.dirty = false;
        self.related_hits.clear();
        self.status = "Note chargée".into();
    }

    pub fn apply_search_hits(&mut self, hits: Vec<NoteSearchHit>) {
        self.search_hits = hits;
        self.status = format!("{} résultat(s)", self.search_hits.len());
    }

    pub fn apply_related(&mut self, hits: Vec<NoteRelatedHit>) {
        self.related_hits = hits;
        self.status = format!("{} note(s) liée(s)", self.related_hits.len());
    }

    pub fn start_new(&mut self) {
        self.selected_path = None;
        self.edit_path = None;
        self.edit_slug = None;
        self.edit_title.clear();
        self.edit_body.clear();
        self.outgoing.clear();
        self.incoming.clear();
        self.related_hits.clear();
        self.is_new = true;
        self.dirty = false;
        self.status = "Nouvelle note".into();
    }

    pub fn mark_saved(&mut self, path: String, slug: String, title: String) {
        self.edit_path = Some(path.clone());
        self.edit_slug = Some(slug);
        self.selected_path = Some(path);
        self.edit_title = title;
        self.is_new = false;
        self.dirty = false;
        self.status = "Enregistré".into();
    }
}

fn split_h1(content: &str) -> (Option<String>, String) {
    let trimmed = content.trim_start();
    if let Some(rest) = trimmed.strip_prefix("# ") {
        let mut lines = rest.lines();
        let title = lines.next().unwrap_or("").trim().to_string();
        let body = lines.collect::<Vec<_>>().join("\n");
        let body = body.trim_start_matches('\n').to_string();
        (Some(title), body)
    } else {
        (None, content.to_string())
    }
}

fn insert_wrap(buf: &mut String, before: &str, after: &str, placeholder: &str) {
    if buf.is_empty() {
        buf.push_str(before);
        buf.push_str(placeholder);
        buf.push_str(after);
    } else {
        if !buf.ends_with('\n') {
            buf.push('\n');
        }
        buf.push_str(before);
        buf.push_str(placeholder);
        buf.push_str(after);
    }
}

/// Dessine l'onglet Notes. Retourne les actions à exécuter.
pub fn show_notes_panel(ui: &mut Ui, state: &mut NotesPanelState) -> NotesActions {
    let mut actions = NotesActions::default();

    ui.heading("Notes");
    ui.horizontal(|ui| {
        if ui.button("Rafraîchir").clicked() {
            actions.list = true;
        }
        ui.text_edit_singleline(&mut state.search_query);
        if ui.button("Rechercher").clicked() && !state.search_query.is_empty() {
            actions.search = Some(state.search_query.clone());
        }
        if ui.button("Nouvelle note").clicked() {
            state.start_new();
        }
        ui.checkbox(&mut state.show_preview, "Aperçu");
    });
    if !state.status.is_empty() {
        ui.weak(&state.status);
    }
    ui.separator();

    ui.columns(2, |cols| {
        // --- Liste ---
        cols[0].vertical(|ui| {
            ui.label(RichText::new("Liste").strong());
            ui.horizontal(|ui| {
                ui.label("Filtrer");
                ui.text_edit_singleline(&mut state.filter);
            });
            egui::ScrollArea::vertical()
                .id_salt("notes_list")
                .max_height(420.0)
                .show(ui, |ui| {
                    let filter = state.filter.to_lowercase();
                    let items: Vec<_> = state
                        .notes
                        .iter()
                        .filter(|n| {
                            filter.is_empty()
                                || n.title.to_lowercase().contains(&filter)
                                || n.excerpt.to_lowercase().contains(&filter)
                        })
                        .cloned()
                        .collect();
                    if items.is_empty() {
                        ui.weak("Aucune note — créez-en une ou rafraîchissez.");
                    }
                    for n in items {
                        let selected = state.selected_path.as_deref() == Some(n.path.as_str());
                        let label = if n.excerpt.is_empty() {
                            n.title.clone()
                        } else {
                            format!("{}\n{}", n.title, n.excerpt)
                        };
                        if ui.selectable_label(selected, label).clicked() {
                            actions.read_path = Some(n.path.clone());
                            // Toujours envoyer aussi le titre : évite les anciens
                            // WASM notes.read qui exigeaient `title` (issue #1).
                            if !n.title.is_empty() {
                                actions.read_title = Some(n.title.clone());
                            }
                        }
                    }
                });

            if !state.search_hits.is_empty() {
                ui.separator();
                ui.label(RichText::new("Recherche").strong());
                egui::ScrollArea::vertical()
                    .id_salt("notes_search")
                    .max_height(160.0)
                    .show(ui, |ui| {
                        for h in state.search_hits.clone() {
                            let title = if h.title.is_empty() {
                                h.text.chars().take(60).collect::<String>()
                            } else {
                                h.title.clone()
                            };
                            if ui
                                .button(format!("{title} ({:.2})", h.score))
                                .on_hover_text(&h.text)
                                .clicked()
                            {
                                if !h.path.is_empty() {
                                    actions.read_path = Some(h.path);
                                } else if !h.title.is_empty() {
                                    actions.read_title = Some(h.title);
                                }
                            }
                        }
                    });
            }
        });

        // --- Éditeur ---
        cols[1].vertical(|ui| {
            ui.label(RichText::new(if state.is_new {
                "Nouvelle note"
            } else {
                "Édition"
            }).strong());

            ui.horizontal(|ui| {
                ui.label("Titre");
                if ui
                    .text_edit_singleline(&mut state.edit_title)
                    .changed()
                {
                    state.dirty = true;
                }
            });

            ui.horizontal_wrapped(|ui| {
                if ui.small_button("H1").clicked() {
                    insert_wrap(&mut state.edit_body, "# ", "\n", "Titre");
                    state.dirty = true;
                }
                if ui.small_button("H2").clicked() {
                    insert_wrap(&mut state.edit_body, "## ", "\n", "Sous-titre");
                    state.dirty = true;
                }
                if ui.small_button("H3").clicked() {
                    insert_wrap(&mut state.edit_body, "### ", "\n", "Section");
                    state.dirty = true;
                }
                if ui.small_button("Gras").clicked() {
                    insert_wrap(&mut state.edit_body, "**", "**", "texte");
                    state.dirty = true;
                }
                if ui.small_button("Italique").clicked() {
                    insert_wrap(&mut state.edit_body, "*", "*", "texte");
                    state.dirty = true;
                }
                if ui.small_button("Liste").clicked() {
                    insert_wrap(&mut state.edit_body, "- ", "\n", "élément");
                    state.dirty = true;
                }
                if ui.small_button("Quote").clicked() {
                    insert_wrap(&mut state.edit_body, "> ", "\n", "citation");
                    state.dirty = true;
                }
                if ui.small_button("Code").clicked() {
                    insert_wrap(&mut state.edit_body, "```\n", "\n```\n", "code");
                    state.dirty = true;
                }
                if ui.small_button("Tableau").clicked() {
                    insert_wrap(
                        &mut state.edit_body,
                        "| A | B |\n| --- | --- |\n| ",
                        " |  |\n",
                        "cellule",
                    );
                    state.dirty = true;
                }
                if ui.small_button("[[lien]]").clicked() {
                    insert_wrap(&mut state.edit_body, "[[", "]]", "Titre note");
                    state.dirty = true;
                }
            });

            let editor = egui::TextEdit::multiline(&mut state.edit_body)
                .desired_rows(12)
                .desired_width(f32::INFINITY)
                .font(egui::TextStyle::Monospace);
            if ui.add(editor).changed() {
                state.dirty = true;
            }

            if state.show_preview {
                ui.separator();
                ui.label(RichText::new("Aperçu").strong());
                let preview = if state.edit_title.is_empty() {
                    state.edit_body.clone()
                } else {
                    format!("# {}\n\n{}", state.edit_title, state.edit_body)
                };
                egui::ScrollArea::vertical()
                    .id_salt("notes_preview")
                    .max_height(180.0)
                    .show(ui, |ui| {
                        CommonMarkViewer::new().show(ui, &mut state.md_cache, &preview);
                    });
            }

            ui.horizontal(|ui| {
                let can_save = !state.edit_title.trim().is_empty();
                if ui
                    .add_enabled(can_save, egui::Button::new(if state.is_new {
                        "Créer"
                    } else {
                        "Enregistrer"
                    }))
                    .clicked()
                {
                    let title = state.edit_title.trim().to_string();
                    let body = state.edit_body.clone();
                    if state.is_new || state.edit_path.is_none() {
                        actions.save_create = Some((title, body));
                    } else {
                        let path = state.edit_path.clone().unwrap_or_default();
                        actions.save_update = Some((title, path, body));
                    }
                }
                if let Some(path) = state.edit_path.clone() {
                    if ui.button("Joindre à un agent").clicked() {
                        actions.attach_path = Some(path);
                    }
                    if ui.button("Notes liées").clicked() {
                        let topic = state.search_query.clone();
                        actions.related = Some((
                            state.edit_path.clone().unwrap_or_default(),
                            topic,
                        ));
                    }
                }
                if state.dirty {
                    ui.weak("• modifié");
                }
            });

            // Liens
            if !state.outgoing.is_empty() || !state.incoming.is_empty() {
                ui.separator();
                ui.label(RichText::new("Liens").strong());
                if !state.outgoing.is_empty() {
                    ui.label("Sortants");
                    for l in state.outgoing.clone() {
                        let mark = if l.exists { "→" } else { "↛" };
                        if ui.button(format!("{mark} {}", l.title)).clicked() && l.exists {
                            actions.read_path = Some(l.path);
                        }
                    }
                }
                if !state.incoming.is_empty() {
                    ui.label("Backlinks");
                    for l in state.incoming.clone() {
                        if ui.button(format!("← {}", l.title)).clicked() {
                            actions.read_path = Some(l.path);
                        }
                    }
                }
            }

            if !state.related_hits.is_empty() {
                ui.separator();
                ui.label(RichText::new("Liées (pertinence)").strong());
                for h in state.related_hits.clone() {
                    if ui
                        .button(format!(
                            "{} [{}] hop{} score {:.2}",
                            h.title, h.relation, h.hops, h.score
                        ))
                        .on_hover_text(&h.excerpt)
                        .clicked()
                    {
                        actions.read_path = Some(h.path);
                    }
                }
            }
        });
    });

    actions
}

/// Parse le résultat `notes.list`.
pub fn parse_list_result(v: &serde_json::Value) -> Vec<NoteListItem> {
    v.get("notes")
        .and_then(|n| n.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    if let Some(path) = item.as_str() {
                        // Ancien format (chemins seuls).
                        let slug = path
                            .rsplit('/')
                            .next()
                            .unwrap_or("")
                            .trim_end_matches(".md")
                            .to_string();
                        return Some(NoteListItem {
                            title: slug.replace('-', " "),
                            path: path.to_string(),
                            slug,
                            excerpt: String::new(),
                        });
                    }
                    serde_json::from_value::<NoteListItem>(item.clone()).ok()
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn parse_detail(v: &serde_json::Value) -> Option<NoteDetail> {
    serde_json::from_value(v.clone()).ok()
}

pub fn parse_search_hits(v: &serde_json::Value) -> Vec<NoteSearchHit> {
    let hits = v
        .get("hits")
        .and_then(|h| h.as_array())
        .cloned()
        .unwrap_or_default();
    hits.into_iter()
        .map(|h| {
            let meta = h.get("metadata").cloned().unwrap_or(serde_json::json!({}));
            NoteSearchHit {
                id: h.get("id").and_then(|x| x.as_u64()).unwrap_or(0),
                text: h
                    .get("text")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string(),
                score: h.get("score").and_then(|x| x.as_f64()).unwrap_or(0.0) as f32,
                title: meta
                    .get("title")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string(),
                path: meta
                    .get("path")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string(),
                slug: meta
                    .get("slug")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string(),
            }
        })
        .collect()
}

pub fn parse_related(v: &serde_json::Value) -> Vec<NoteRelatedHit> {
    v.get("related")
        .and_then(|r| serde_json::from_value::<Vec<NoteRelatedHit>>(r.clone()).ok())
        .unwrap_or_default()
}
