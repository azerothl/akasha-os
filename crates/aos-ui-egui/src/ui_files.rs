//! Onglet Fichiers (S5) : même store logique que les agents (`fs.*`).
//!
//! Navigation par préfixes (pas de dossiers serveur), aperçu/édition texte
//! (200 ko max), badges de sensibilité (`fs.set_class`, cap `fs.reclassify`),
//! suppression confirmée (version archivée côté serveur), renommage
//! read+write+delete (documenté non-atomique).

use crate::cmd::Cmd;
use crate::{agent_panel, i18n, UiApp};
use aos_proto::DataClass;
use eframe::egui;

/// Au-delà, pas d'éditeur inline (aperçu refusé, lecture impossible).
const PREVIEW_MAX_CHARS: usize = 200_000;

fn class_label(t: &i18n::UiStrings, class: DataClass) -> &'static str {
    match class {
        DataClass::Public => t.files_class_public,
        DataClass::Private => t.files_class_private,
        DataClass::Secret => t.files_class_secret,
    }
}

fn class_color(ui: &egui::Ui, class: DataClass) -> egui::Color32 {
    let theme_c = crate::theme::button_colors(ui);
    match class {
        DataClass::Public => theme_c.success,
        DataClass::Private => ui.visuals().weak_text_color(),
        DataClass::Secret => theme_c.danger,
    }
}

impl UiApp {
    pub(crate) fn ui_files(&mut self, ui: &mut egui::Ui) {
        let t = i18n::strings(&self.prefs.language);
        ui.heading(t.tab_files);
        ui.weak(t.tab_hint_files);
        ui.weak(t.files_only_indexed);
        ui.separator();

        // Fil d'Ariane + recherche + création.
        ui.horizontal_wrapped(|ui| {
            for (prefix, label) in self.files_ui.crumbs() {
                if ui.small_button(label).clicked() {
                    self.files_ui.prefix = prefix;
                }
            }
        });
        ui.horizontal(|ui| {
            crate::ui_primitives::search_field(ui, &mut self.files_ui.filter, t.files_search);
        });
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.files_ui.new_name)
                    .desired_width(220.0)
                    .hint_text(t.files_new_hint),
            );
            if ui.button(t.files_new).clicked() {
                let name = self.files_ui.new_name.trim().replace('\\', "/");
                if !name.is_empty() && !name.ends_with('/') {
                    let path = format!("{}{}", self.files_ui.prefix, name);
                    let _ = self.cmd_tx.send(Cmd::FilesWrite {
                        path: path.clone(),
                        content: String::new(),
                    });
                    // Lecture immédiate après écriture (runtime séquentiel).
                    let _ = self.cmd_tx.send(Cmd::FilesRead { path });
                    self.files_ui.new_name.clear();
                }
            }
            if ui.small_button(t.caps_refresh).clicked() {
                let _ = self.cmd_tx.send(Cmd::FilesList {
                    prefix: String::new(),
                });
            }
        });
        ui.separator();

        let total_w = ui.available_width();
        let left_w = (total_w * 0.38).clamp(180.0, 340.0);
        ui.horizontal(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(left_w, ui.available_height()),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_max_width(left_w);
                    self.ui_files_list(ui, &t);
                },
            );
            ui.separator();
            ui.allocate_ui_with_layout(
                egui::vec2((total_w - left_w - 12.0).max(120.0), ui.available_height()),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    self.ui_files_viewer(ui, &t);
                },
            );
        });

        self.ui_files_rename_popup(ui.ctx(), &t);
        self.ui_files_delete_popup(ui.ctx(), &t);
    }

    fn ui_files_list(&mut self, ui: &mut egui::Ui, t: &i18n::UiStrings) {
        let dirs = self.files_ui.child_dirs();
        for dir in &dirs {
            let label = format!("▸ {dir}/");
            if ui.selectable_label(false, label).clicked() {
                self.files_ui.prefix = format!("{}{dir}/", self.files_ui.prefix);
            }
        }
        // Copie possédée : les menus contextuels empruntent `self` en mutable.
        let rows: Vec<(String, DataClass, u64, u64)> = self
            .files_ui
            .child_files()
            .iter()
            .map(|e| (e.path.clone(), e.class, e.version, e.size_bytes))
            .collect();
        if rows.is_empty() && dirs.is_empty() {
            ui.weak(t.files_empty);
            return;
        }
        enum RowAct {
            Open(String),
            Rename(String),
            Delete(String),
        }
        let mut act: Option<RowAct> = None;
        for (path, class, version, size) in &rows {
            let selected = self.files_ui.open_path.as_deref() == Some(path.as_str());
            let short =
                agent_panel::truncate(path.strip_prefix(&self.files_ui.prefix).unwrap_or(path), 30);
            let row = ui.horizontal(|ui| {
                ui.colored_label(
                    class_color(ui, *class),
                    format!("[{}]", class_label(t, *class)),
                );
                let title = ui
                    .selectable_label(selected, short)
                    .on_hover_text(path.as_str());
                ui.weak(format!(
                    "{} · {}",
                    crate::ui_format::human_bytes(*size),
                    t.files_version.replace("{version}", &version.to_string()),
                ));
                title
            });
            if row.inner.clicked() || row.response.clicked() {
                act = Some(RowAct::Open(path.clone()));
            }
            row.response.context_menu(|ui| {
                if ui.button(t.files_open).clicked() {
                    act = Some(RowAct::Open(path.clone()));
                    ui.close_menu();
                }
                if ui.button(t.files_rename).clicked() {
                    act = Some(RowAct::Rename(path.clone()));
                    ui.close_menu();
                }
                if ui.button(t.files_delete).clicked() {
                    act = Some(RowAct::Delete(path.clone()));
                    ui.close_menu();
                }
            });
            if act.is_some() {
                break;
            }
        }
        match act {
            Some(RowAct::Open(path)) => {
                let _ = self.cmd_tx.send(Cmd::FilesRead { path });
            }
            Some(RowAct::Rename(path)) => {
                self.files_ui.rename_target = path
                    .strip_prefix(&self.files_ui.prefix)
                    .unwrap_or(&path)
                    .to_string();
                self.files_ui.rename_source = Some(path);
                self.files_ui.rename_open = true;
            }
            Some(RowAct::Delete(path)) => {
                self.files_ui.delete_confirm = Some(path);
            }
            None => {}
        }
    }

    fn ui_files_viewer(&mut self, ui: &mut egui::Ui, t: &i18n::UiStrings) {
        let Some(path) = self.files_ui.open_path.clone() else {
            ui.weak(t.files_empty);
            return;
        };
        ui.horizontal_wrapped(|ui| {
            ui.strong(&path);
            ui.colored_label(
                class_color(ui, self.files_ui.open_class),
                format!("[{}]", class_label(t, self.files_ui.open_class)),
            );
            ui.weak(
                t.files_version
                    .replace("{version}", &self.files_ui.open_version.to_string()),
            );
        });
        ui.horizontal_wrapped(|ui| {
            // Cycle Public → Privé → Secret (cap `fs.reclassify` côté serveur).
            let next = match self.files_ui.open_class {
                DataClass::Public => DataClass::Private,
                DataClass::Private => DataClass::Secret,
                DataClass::Secret => DataClass::Public,
            };
            if ui
                .small_button(class_label(t, next))
                .on_hover_text(if self.prefs.language == "fr" {
                    "Changer la sensibilité"
                } else {
                    "Change sensitivity"
                })
                .clicked()
            {
                let _ = self.cmd_tx.send(Cmd::FilesSetClass {
                    path: path.clone(),
                    class: next,
                });
            }
            if self.files_ui.open_dirty && ui.button(t.files_save).clicked() {
                let _ = self.cmd_tx.send(Cmd::FilesWrite {
                    path: path.clone(),
                    content: self.files_ui.open_content.clone(),
                });
                self.files_ui.open_dirty = false;
            }
            if ui.small_button(t.files_rename).clicked() {
                self.files_ui.rename_target = path
                    .strip_prefix(&self.files_ui.prefix)
                    .unwrap_or(&path)
                    .to_string();
                self.files_ui.rename_source = Some(path.clone());
                self.files_ui.rename_open = true;
            }
            if ui.small_button(t.files_delete).clicked() {
                self.files_ui.delete_confirm = Some(path.clone());
            }
        });
        ui.separator();
        if self.files_ui.open_content.chars().count() > PREVIEW_MAX_CHARS {
            ui.weak(t.files_preview_binary);
            return;
        }
        let resp = ui.add_sized(
            egui::vec2(ui.available_width(), ui.available_height().max(120.0)),
            egui::TextEdit::multiline(&mut self.files_ui.open_content)
                .font(egui::TextStyle::Monospace)
                .code_editor(),
        );
        if resp.changed() {
            self.files_ui.open_dirty = true;
        }
    }

    fn ui_files_rename_popup(&mut self, ctx: &egui::Context, t: &i18n::UiStrings) {
        if !self.files_ui.rename_open {
            return;
        }
        let mut close = false;
        let mut apply: Option<String> = None;
        egui::Window::new(t.files_rename_title)
            .collapsible(false)
            .resizable(false)
            .default_width(320.0)
            .show(ctx, |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.files_ui.rename_target)
                        .desired_width(f32::INFINITY)
                        .hint_text(t.files_new_hint),
                );
                ui.horizontal(|ui| {
                    if ui.button(t.memory_btn_cancel).clicked() {
                        close = true;
                    }
                    if ui.button(t.files_rename).clicked()
                        && !self.files_ui.rename_target.trim().is_empty()
                    {
                        apply = Some(self.files_ui.rename_target.trim().replace('\\', "/"));
                    }
                });
            });
        if let Some(target) = apply {
            // Non-atomique assumé : read → write → delete (versions serveur).
            let to = format!("{}{}", self.files_ui.prefix, target);
            let from = self.files_ui.rename_source.clone().unwrap_or_default();
            if !from.is_empty() && from != to {
                if self.files_ui.open_path.as_deref() == Some(from.as_str()) {
                    let content = self.files_ui.open_content.clone();
                    let _ = self.cmd_tx.send(Cmd::FilesWrite { path: to, content });
                    let _ = self.cmd_tx.send(Cmd::FilesDelete { path: from });
                } else {
                    // Contenu inconnu : lecture d'abord, suite dans on_read.
                    self.files_ui.pending_rename = Some((from.clone(), to));
                    let _ = self.cmd_tx.send(Cmd::FilesRead { path: from });
                }
            }
            self.files_ui.rename_open = false;
            self.files_ui.rename_source = None;
        } else if close {
            self.files_ui.rename_open = false;
            self.files_ui.rename_source = None;
        }
    }

    fn ui_files_delete_popup(&mut self, ctx: &egui::Context, t: &i18n::UiStrings) {
        let Some(id) = self.files_ui.delete_confirm.clone() else {
            return;
        };
        let mut decision = None;
        egui::Window::new(t.files_delete_title)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(t.files_delete_body.replace("{path}", &id));
                ui.horizontal(|ui| {
                    if ui.button(t.memory_btn_cancel).clicked() {
                        decision = Some(false);
                    }
                    if ui.button(t.files_delete).clicked() {
                        decision = Some(true);
                    }
                });
            });
        if let Some(confirm) = decision {
            self.files_ui.delete_confirm = None;
            if confirm {
                if self.files_ui.open_path.as_deref() == Some(id.as_str()) {
                    self.files_ui.open_path = None;
                    self.files_ui.open_content.clear();
                }
                let _ = self.cmd_tx.send(Cmd::FilesDelete { path: id });
            }
        }
    }
}
