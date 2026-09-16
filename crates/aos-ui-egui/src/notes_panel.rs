//! Onglet Notes — liste, éditeur markdown, aperçu, liens, joindre à un agent.

use eframe::egui::{self, RichText, Ui};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use serde::{Deserialize, Serialize};

use crate::i18n::UiStrings;
use crate::icons;
use crate::ui_primitives;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteListItem {
    pub title: String,
    pub path: String,
    pub slug: String,
    #[serde(default)]
    pub excerpt: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub updated_seq: u64,
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
    pub tags: Vec<String>,
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
    pub delete_path: Option<String>,
    pub attach_path: Option<String>,
    pub related: Option<(String, String)>, // path, topic
    pub retry_save: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotesSort {
    Recent,
    TitleAsc,
    TitleDesc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotesTagFilter {
    All,
    Untagged,
    Tag(String),
}

pub struct NotesPanelState {
    pub notes: Vec<NoteListItem>,
    pub filter: String,
    pub search_query: String,
    pub sort: NotesSort,
    pub tag_filter: NotesTagFilter,
    pub search_hits: Vec<NoteSearchHit>,
    pub related_hits: Vec<NoteRelatedHit>,
    pub selected_path: Option<String>,
    pub edit_title: String,
    pub edit_tags: String,
    pub edit_body: String,
    pub edit_path: Option<String>,
    pub edit_slug: Option<String>,
    pub is_new: bool,
    pub dirty: bool,
    pub outgoing: Vec<NoteLink>,
    pub incoming: Vec<NoteLink>,
    pub status: String,
    pub show_preview: bool,
    pub preview_mode: NotesPreviewMode,
    /// Locked chrome when the last create/save failed.
    pub create_failed: bool,
    /// Last create/save payload for retry (title, body, optional path for update).
    pub retry_payload: Option<(String, String, Option<String>)>,
    md_cache: CommonMarkCache,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotesPreviewMode {
    Editor,
    Preview,
    Both,
}

impl Default for NotesPanelState {
    fn default() -> Self {
        Self {
            notes: Vec::new(),
            filter: String::new(),
            search_query: String::new(),
            sort: NotesSort::Recent,
            tag_filter: NotesTagFilter::All,
            search_hits: Vec::new(),
            related_hits: Vec::new(),
            selected_path: None,
            edit_title: String::new(),
            edit_tags: String::new(),
            edit_body: String::new(),
            edit_path: None,
            edit_slug: None,
            is_new: true,
            dirty: false,
            outgoing: Vec::new(),
            incoming: Vec::new(),
            status: String::new(),
            show_preview: true,
            preview_mode: NotesPreviewMode::Both,
            create_failed: false,
            retry_payload: None,
            md_cache: CommonMarkCache::default(),
        }
    }
}

impl NotesPanelState {
    pub fn can_save(&self) -> bool {
        !self.edit_title.trim().is_empty()
    }

    pub fn apply_listed(&mut self, notes: Vec<NoteListItem>, count_tpl: &str) {
        self.notes = notes;
        if self.create_failed && !self.notes.is_empty() {
            self.create_failed = false;
            self.retry_payload = None;
        }
        self.status = count_tpl.replace("{n}", &self.notes.len().to_string());
    }

    pub fn apply_loaded(&mut self, detail: NoteDetail) {
        self.selected_path = Some(detail.path.clone());
        self.edit_path = Some(detail.path);
        self.edit_slug = Some(detail.slug);
        self.edit_title = detail.title;
        let raw_body = if detail.body.is_empty() {
            let (_, body) = split_h1(&detail.content);
            body
        } else {
            detail.body
        };
        let (fm_tags, body) = split_note_frontmatter(&raw_body);
        self.edit_body = body;
        self.edit_tags = format_tags_input(if detail.tags.is_empty() {
            &fm_tags
        } else {
            &detail.tags
        });
        self.outgoing = detail.outgoing;
        self.incoming = detail.incoming;
        self.is_new = false;
        self.dirty = false;
        self.related_hits.clear();
        self.create_failed = false;
        self.retry_payload = None;
        self.status = String::new();
    }

    pub fn apply_search_hits(&mut self, hits: Vec<NoteSearchHit>, count_tpl: &str) {
        self.search_hits = hits;
        self.status = count_tpl.replace("{n}", &self.search_hits.len().to_string());
    }

    pub fn apply_related(&mut self, hits: Vec<NoteRelatedHit>, count_tpl: &str) {
        self.related_hits = hits;
        self.status = count_tpl.replace("{n}", &self.related_hits.len().to_string());
    }

    pub fn start_new(&mut self) {
        self.selected_path = None;
        self.edit_path = None;
        self.edit_slug = None;
        self.edit_title.clear();
        self.edit_tags.clear();
        self.edit_body.clear();
        self.outgoing.clear();
        self.incoming.clear();
        self.related_hits.clear();
        self.is_new = true;
        self.dirty = false;
        self.create_failed = false;
        self.retry_payload = None;
        self.status.clear();
    }

    pub fn mark_saved(&mut self, path: String, slug: String, title: String, saved_label: &str) {
        self.edit_path = Some(path.clone());
        self.edit_slug = Some(slug);
        self.selected_path = Some(path);
        self.edit_title = title;
        self.is_new = false;
        self.dirty = false;
        self.create_failed = false;
        self.retry_payload = None;
        self.status = saved_label.to_string();
    }

    pub fn mark_save_failed(&mut self, title: String, body: String, path: Option<String>) {
        self.create_failed = true;
        self.retry_payload = Some((title, body, path));
        self.status.clear();
    }

    pub fn take_retry_action(&mut self) -> Option<NotesActions> {
        let (title, body, path) = self.retry_payload.clone()?;
        let mut actions = NotesActions::default();
        if let Some(path) = path {
            actions.save_update = Some((title, path, body));
        } else {
            actions.save_create = Some((title, body));
        }
        self.create_failed = false;
        Some(actions)
    }

    pub fn apply_deleted(&mut self, path: &str, deleted_label: &str) {
        self.notes.retain(|n| n.path != path);
        if self.selected_path.as_deref() == Some(path) || self.edit_path.as_deref() == Some(path) {
            self.start_new();
        }
        self.status = deleted_label.to_string();
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

fn split_note_frontmatter(content: &str) -> (Vec<String>, String) {
    let bom_stripped = content.strip_prefix('\u{feff}').unwrap_or(content);
    let leading = bom_stripped.trim_start_matches(['\r', '\n']);
    let Some(after_open) = leading.strip_prefix("---") else {
        return (Vec::new(), content.to_string());
    };
    let after_open = after_open.strip_prefix('\r').unwrap_or(after_open);
    let Some(after_open) = after_open.strip_prefix('\n') else {
        return (Vec::new(), content.to_string());
    };
    let Some(close_idx) = after_open.find("\n---") else {
        return (Vec::new(), content.to_string());
    };
    let yaml = &after_open[..close_idx];
    let mut after = &after_open[close_idx + "\n---".len()..];
    after = after.strip_prefix('\r').unwrap_or(after);
    after = after.strip_prefix('\n').unwrap_or(after);
    after = after.trim_start_matches(['\r', '\n']);
    let mut tags = Vec::new();
    for line in yaml.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("tags:") {
            tags = parse_tags_input(rest.trim_start_matches('[').trim_end_matches(']'));
        }
    }
    (tags, after.to_string())
}

pub fn parse_tags_input(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for part in raw.split([',', ';']) {
        let t = part.trim();
        if t.is_empty() {
            continue;
        }
        let key = t.to_lowercase();
        if !seen.insert(key) {
            continue;
        }
        out.push(t.to_string());
        if out.len() >= 16 {
            break;
        }
    }
    out
}

fn format_tags_input(tags: &[String]) -> String {
    tags.join(", ")
}

fn compose_note_content(tags: &[String], body: &str) -> String {
    format!("---\ntags: {}\n---\n\n{}", tags.join(", "), body)
}

fn collect_unique_tags(notes: &[NoteListItem]) -> Vec<String> {
    let mut tags = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for n in notes {
        for tag in &n.tags {
            let key = tag.to_lowercase();
            if seen.insert(key) {
                tags.push(tag.clone());
            }
        }
    }
    tags.sort_by_key(|a| a.to_lowercase());
    tags
}

pub fn visible_notes<'a>(
    notes: &'a [NoteListItem],
    query: &str,
    tag_filter: &NotesTagFilter,
    sort: NotesSort,
) -> Vec<&'a NoteListItem> {
    let q = query.trim().to_lowercase();
    let mut items: Vec<&NoteListItem> = notes
        .iter()
        .filter(|n| note_matches_filters(n, &q, tag_filter))
        .collect();
    match sort {
        NotesSort::Recent => items.sort_by(|a, b| {
            b.updated_seq
                .cmp(&a.updated_seq)
                .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
        }),
        NotesSort::TitleAsc => items.sort_by_key(|a| a.title.to_lowercase()),
        NotesSort::TitleDesc => items.sort_by_key(|a| std::cmp::Reverse(a.title.to_lowercase())),
    }
    items
}

fn note_matches_filters(n: &NoteListItem, query: &str, tag_filter: &NotesTagFilter) -> bool {
    let tag_ok = match tag_filter {
        NotesTagFilter::All => true,
        NotesTagFilter::Untagged => n.tags.is_empty(),
        NotesTagFilter::Tag(tag) => n.tags.iter().any(|t| t.eq_ignore_ascii_case(tag)),
    };
    if !tag_ok {
        return false;
    }
    if query.is_empty() {
        return true;
    }
    n.title.to_lowercase().contains(query)
        || n.excerpt.to_lowercase().contains(query)
        || n.slug.to_lowercase().contains(query)
        || n.tags.iter().any(|t| t.to_lowercase().contains(query))
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
pub fn show_notes_panel(ui: &mut Ui, state: &mut NotesPanelState, t: &UiStrings) -> NotesActions {
    let mut actions = NotesActions::default();

    ui.heading(t.tab_notes);
    ui.horizontal(|ui| {
        if ui.button(t.decl_ui_refresh).clicked() {
            actions.list = true;
        }
        if ui.button(t.notes_new).clicked() {
            state.start_new();
        }
        ui.label(t.notes_view_label);
        for (mode, label) in [
            (NotesPreviewMode::Editor, t.notes_view_editor),
            (NotesPreviewMode::Preview, t.notes_view_preview),
            (NotesPreviewMode::Both, t.notes_view_both),
        ] {
            if ui
                .selectable_label(state.preview_mode == mode, label)
                .clicked()
            {
                state.preview_mode = mode;
                state.show_preview = mode != NotesPreviewMode::Editor;
            }
        }
    });
    if !state.status.is_empty() {
        ui.weak(&state.status);
    }
    ui.separator();

    let editor_h = ui.available_height();
    let list_h = editor_h * 0.55;
    let search_h = editor_h * 0.25;
    egui::SidePanel::left("notes_list_panel")
        .default_width((ui.available_width() * 0.34).clamp(280.0, 520.0))
        .min_width(240.0)
        .max_width(560.0)
        .resizable(true)
        .show_inside(ui, |list_ui| {
            list_ui.vertical(|ui| {
                ui.label(RichText::new(t.notes_list).strong());
                let find = ui_primitives::search_field(
                    ui,
                    &mut state.filter,
                    t.notes_filter,
                    t.notes_find_hint,
                    t.search_field_clear,
                );
                if find.has_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                    && !state.filter.trim().is_empty()
                {
                    actions.search = Some(state.filter.clone());
                    state.search_query = state.filter.clone();
                }
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !state.filter.trim().is_empty(),
                            egui::Button::new(t.notes_search),
                        )
                        .clicked()
                    {
                        actions.search = Some(state.filter.clone());
                        state.search_query = state.filter.clone();
                    }
                    ui.label(t.notes_sort);
                    let sort_label = match state.sort {
                        NotesSort::Recent => t.notes_sort_recent,
                        NotesSort::TitleAsc => t.notes_sort_title,
                        NotesSort::TitleDesc => t.notes_sort_title_desc,
                    };
                    egui::ComboBox::from_id_salt("notes_sort")
                        .selected_text(sort_label)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut state.sort,
                                NotesSort::Recent,
                                t.notes_sort_recent,
                            );
                            ui.selectable_value(
                                &mut state.sort,
                                NotesSort::TitleAsc,
                                t.notes_sort_title,
                            );
                            ui.selectable_value(
                                &mut state.sort,
                                NotesSort::TitleDesc,
                                t.notes_sort_title_desc,
                            );
                        });
                });
                let known_tags = collect_unique_tags(&state.notes);
                let has_untagged = state.notes.iter().any(|n| n.tags.is_empty());
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .selectable_label(
                            matches!(state.tag_filter, NotesTagFilter::All),
                            t.notes_tag_all,
                        )
                        .clicked()
                    {
                        state.tag_filter = NotesTagFilter::All;
                    }
                    if has_untagged
                        && ui
                            .selectable_label(
                                matches!(state.tag_filter, NotesTagFilter::Untagged),
                                t.notes_untagged,
                            )
                            .clicked()
                    {
                        state.tag_filter = NotesTagFilter::Untagged;
                    }
                    for tag in &known_tags {
                        let selected =
                            matches!(&state.tag_filter, NotesTagFilter::Tag(t) if t == tag);
                        if ui.selectable_label(selected, tag).clicked() {
                            state.tag_filter = NotesTagFilter::Tag(tag.clone());
                        }
                    }
                });
                let visible =
                    visible_notes(&state.notes, &state.filter, &state.tag_filter, state.sort);
                let shown = visible.len();
                let total = state.notes.len();
                if shown != total {
                    ui.weak(
                        t.notes_filtered_count
                            .replace("{shown}", &shown.to_string())
                            .replace("{total}", &total.to_string()),
                    );
                }
                let items: Vec<NoteListItem> = visible.into_iter().cloned().collect();
                egui::ScrollArea::vertical()
                    .id_salt("notes_list")
                    .max_height(list_h.max(120.0))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if items.is_empty() {
                            ui.weak(if state.notes.is_empty() {
                                t.notes_empty
                            } else {
                                t.notes_empty_filter
                            });
                        }
                        for n in items {
                            let selected = state.selected_path.as_deref() == Some(n.path.as_str());
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    let title = RichText::new(&n.title).strong();
                                    if ui.selectable_label(selected, title).clicked() {
                                        actions.read_path = Some(n.path.clone());
                                        if !n.title.is_empty() {
                                            actions.read_title = Some(n.title.clone());
                                        }
                                    }
                                    if !n.excerpt.is_empty() {
                                        ui.weak(&n.excerpt);
                                    }
                                    if !n.tags.is_empty() {
                                        ui.weak(n.tags.join(" · "));
                                    }
                                });
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::TOP),
                                    |ui| {
                                        if ui_primitives::danger_confirm_button(
                                            ui,
                                            ("notes_del", n.path.as_str()),
                                            t.notes_delete,
                                            t.notes_delete_confirm,
                                        ) {
                                            actions.delete_path = Some(n.path.clone());
                                        }
                                    },
                                );
                            });
                            ui.separator();
                        }
                    });

                if !state.search_hits.is_empty() {
                    ui.separator();
                    ui.label(RichText::new(t.notes_search).strong());
                    egui::ScrollArea::vertical()
                        .id_salt("notes_search")
                        .max_height(search_h.max(80.0))
                        .auto_shrink([false, false])
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
        });

    // --- Éditeur ---
    ui.vertical(|ui| {
        let preview_h = (editor_h * 0.35).max(100.0);
        ui.label(
            RichText::new(if state.is_new {
                t.notes_editor_new
            } else {
                t.notes_editor_edit
            })
            .strong(),
        );

        let preview_only = state.preview_mode == NotesPreviewMode::Preview;
        if !preview_only {
            if state.create_failed {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(t.notes_create_failed).strong());
                    if ui.button(t.notes_create_retry).clicked() {
                        actions.retry_save = true;
                    }
                });
                ui.separator();
            }

            let can_save = state.can_save();
            ui.horizontal(|ui| {
                ui.label(t.notes_title_label);
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut state.edit_title)
                            .desired_width(180.0)
                            .hint_text(t.notes_title_hint),
                    )
                    .changed()
                {
                    state.dirty = true;
                }
                if ui
                    .add_enabled(
                        can_save,
                        egui::Button::new(if state.is_new {
                            t.tasks_create
                        } else {
                            t.memory_btn_save
                        }),
                    )
                    .clicked()
                {
                    let title = state.edit_title.trim().to_string();
                    let tags = parse_tags_input(&state.edit_tags);
                    let body = compose_note_content(&tags, &state.edit_body);
                    if state.is_new || state.edit_path.is_none() {
                        actions.save_create = Some((title, body));
                    } else {
                        let path = state.edit_path.clone().unwrap_or_default();
                        actions.save_update = Some((title, path, body));
                    }
                }
                if let Some(path) = state.edit_path.clone() {
                    if ui.button(t.notes_attach).clicked() {
                        actions.attach_path = Some(path.clone());
                    }
                    if ui.button(t.notes_related).clicked() {
                        let topic = if state.filter.is_empty() {
                            state.search_query.clone()
                        } else {
                            state.filter.clone()
                        };
                        actions.related = Some((path.clone(), topic));
                    }
                    if ui_primitives::danger_confirm_button(
                        ui,
                        ("notes_del_editor", path.as_str()),
                        t.notes_delete,
                        t.notes_delete_confirm,
                    ) {
                        actions.delete_path = Some(path);
                    }
                }
                if state.dirty {
                    ui.weak("•");
                }
            });

            ui.horizontal(|ui| {
                ui.label(t.notes_tags);
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut state.edit_tags)
                            .desired_width(280.0)
                            .hint_text(t.notes_tags_hint),
                    )
                    .changed()
                {
                    state.dirty = true;
                }
            });

            ui.horizontal_wrapped(|ui| {
                if ui.small_button(t.notes_md_h1).clicked() {
                    insert_wrap(&mut state.edit_body, "# ", "\n", t.notes_md_ph_title);
                    state.dirty = true;
                }
                if ui.small_button(t.notes_md_h2).clicked() {
                    insert_wrap(&mut state.edit_body, "## ", "\n", t.notes_md_ph_subtitle);
                    state.dirty = true;
                }
                if ui.small_button(t.notes_md_h3).clicked() {
                    insert_wrap(&mut state.edit_body, "### ", "\n", t.notes_md_ph_section);
                    state.dirty = true;
                }
                if ui.small_button(t.notes_md_bold).clicked() {
                    insert_wrap(&mut state.edit_body, "**", "**", t.notes_md_ph_text);
                    state.dirty = true;
                }
                if ui.small_button(t.notes_md_italic).clicked() {
                    insert_wrap(&mut state.edit_body, "*", "*", t.notes_md_ph_text);
                    state.dirty = true;
                }
                if ui.small_button(t.notes_md_list).clicked() {
                    insert_wrap(&mut state.edit_body, "- ", "\n", t.notes_md_ph_item);
                    state.dirty = true;
                }
                if ui.small_button(t.notes_md_quote).clicked() {
                    insert_wrap(&mut state.edit_body, "> ", "\n", t.notes_md_ph_quote);
                    state.dirty = true;
                }
                if ui.small_button(t.notes_md_code).clicked() {
                    insert_wrap(&mut state.edit_body, "```\n", "\n```\n", t.notes_md_ph_code);
                    state.dirty = true;
                }
                if ui.small_button(t.notes_md_table).clicked() {
                    insert_wrap(
                        &mut state.edit_body,
                        "| A | B |\n| --- | --- |\n| ",
                        " |  |\n",
                        t.notes_md_ph_cell,
                    );
                    state.dirty = true;
                }
                if ui.small_button(t.notes_md_link).clicked() {
                    insert_wrap(&mut state.edit_body, "[[", "]]", t.notes_md_ph_note_link);
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
        }

        if state.preview_mode != NotesPreviewMode::Editor {
            ui.separator();
            ui.label(RichText::new(t.notes_preview).strong());
            let preview = if state.edit_title.is_empty() {
                state.edit_body.clone()
            } else {
                format!("# {}\n\n{}", state.edit_title, state.edit_body)
            };
            egui::ScrollArea::vertical()
                .id_salt("notes_preview")
                .max_height(preview_h)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    CommonMarkViewer::new().show(ui, &mut state.md_cache, &preview);
                });
        }

        // Liens
        if !state.outgoing.is_empty() || !state.incoming.is_empty() {
            ui.separator();
            ui.label(RichText::new(t.notes_links_header).strong());
            if !state.outgoing.is_empty() {
                ui.label(t.notes_outgoing);
                for l in state.outgoing.clone() {
                    ui.horizontal(|ui| {
                        if l.exists {
                            icons::link_outgoing(ui);
                        } else {
                            icons::link_broken(ui);
                        }
                        if ui.button(&l.title).clicked() && l.exists {
                            actions.read_path = Some(l.path);
                        }
                    });
                }
            }
            if !state.incoming.is_empty() {
                ui.label(t.notes_backlinks);
                for l in state.incoming.clone() {
                    ui.horizontal(|ui| {
                        icons::link_backlink(ui);
                        if ui.button(&l.title).clicked() {
                            actions.read_path = Some(l.path);
                        }
                    });
                }
            }
        }

        if !state.related_hits.is_empty() {
            ui.separator();
            ui.label(RichText::new(t.notes_related_relevance).strong());
            let related_h = ui.available_height().max(80.0);
            egui::ScrollArea::vertical()
                .id_salt("notes_related")
                .max_height(related_h)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for h in state.related_hits.clone() {
                        let label = crate::i18n::format_related_note_label(
                            t,
                            &h.title,
                            &h.relation,
                            h.hops,
                            h.score,
                        );
                        if ui.button(label).on_hover_text(&h.excerpt).clicked() {
                            actions.read_path = Some(h.path);
                        }
                    }
                });
        }
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
                            tags: Vec::new(),
                            updated_seq: 0,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_save_requires_non_empty_title() {
        let mut state = NotesPanelState::default();
        assert!(!state.can_save());
        state.edit_title = "  cohort  ".into();
        assert!(state.can_save());
    }

    #[test]
    fn mark_save_failed_stores_retry_payload() {
        let mut state = NotesPanelState::default();
        state.mark_save_failed("t".into(), "body".into(), None);
        assert!(state.create_failed);
        let retry = state.take_retry_action().expect("retry");
        assert_eq!(
            retry.save_create.as_ref().map(|(t, _)| t.as_str()),
            Some("t")
        );
    }

    fn sample_note(title: &str, tags: &[&str], seq: u64) -> NoteListItem {
        NoteListItem {
            title: title.into(),
            path: format!("/documents/notes/{}.md", title.to_lowercase()),
            slug: title.to_lowercase(),
            excerpt: format!("{title} excerpt"),
            tags: tags.iter().map(|s| (*s).to_string()).collect(),
            updated_seq: seq,
        }
    }

    #[test]
    fn visible_notes_filters_sorts_and_tags() {
        let notes = vec![
            sample_note("Beta", &["travail"], 1),
            sample_note("Alpha", &["idées"], 3),
            sample_note("Gamma", &[], 2),
        ];
        let recent = visible_notes(&notes, "", &NotesTagFilter::All, NotesSort::Recent);
        assert_eq!(
            recent.iter().map(|n| n.title.as_str()).collect::<Vec<_>>(),
            vec!["Alpha", "Gamma", "Beta"]
        );
        let tagged = visible_notes(
            &notes,
            "",
            &NotesTagFilter::Tag("travail".into()),
            NotesSort::TitleAsc,
        );
        assert_eq!(tagged.len(), 1);
        assert_eq!(tagged[0].title, "Beta");
        let found = visible_notes(&notes, "idée", &NotesTagFilter::All, NotesSort::TitleAsc);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].title, "Alpha");
        let untagged = visible_notes(&notes, "", &NotesTagFilter::Untagged, NotesSort::TitleAsc);
        assert_eq!(untagged.len(), 1);
        assert_eq!(untagged[0].title, "Gamma");
    }

    #[test]
    fn apply_deleted_clears_open_note() {
        let mut state = NotesPanelState {
            notes: vec![sample_note("Alpha", &[], 1)],
            selected_path: Some("/documents/notes/alpha.md".into()),
            edit_path: Some("/documents/notes/alpha.md".into()),
            is_new: false,
            ..Default::default()
        };
        state.apply_deleted("/documents/notes/alpha.md", "Deleted");
        assert!(state.notes.is_empty());
        assert!(state.is_new);
        assert_eq!(state.status, "Deleted");
    }

    #[test]
    fn parse_tags_input_dedups() {
        assert_eq!(
            parse_tags_input(" Travail, travail, idées "),
            vec!["Travail", "idées"]
        );
    }

    #[test]
    fn format_related_note_label_uses_i18n_templates() {
        let en = crate::i18n::strings("en");
        let label = crate::i18n::format_related_note_label(&en, "Note A", "out", 2, 0.75);
        assert_eq!(label, "Note A [out] · 2 hops · relevance 0.75");
        assert!(!label.contains("hop2"));
        assert!(!label.contains("score"));
    }
}
