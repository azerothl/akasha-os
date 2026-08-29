//! Prepared research documents: clamped overlay viewer + recoverable list (More → Documents).

use aos_agent::document_index::ResearchDocumentEntry;
use aos_proto::ChatAttachment;
use eframe::egui;
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use std::path::PathBuf;

use crate::decl_ui;
use crate::i18n::UiStrings;
use crate::os_open::aos_home;

#[derive(Debug, Clone, Default)]
pub struct DocumentOverlayState {
    pub open: bool,
    pub path: String,
    pub title: String,
}

#[derive(Debug, Clone, Default)]
pub struct DocumentsListState {
    pub open: bool,
}

pub fn overlay_window_sizes(avail_w: f32, avail_h: f32) -> ([f32; 2], [f32; 2], [f32; 2]) {
    let margin = 16.0_f32;
    let max_w = (avail_w - margin * 2.0).max(280.0_f32);
    let max_h = (avail_h - margin * 2.0).max(200.0_f32);
    let default_size = [760.0_f32.min(max_w), 520.0_f32.min(max_h)];
    let max_size = [max_w, max_h];
    let min_size = [280.0_f32.min(max_w), 200.0_f32.min(max_h)];
    (default_size, max_size, min_size)
}

pub fn open_document(overlay: &mut DocumentOverlayState, question: &str, path: &str) {
    overlay.open = true;
    overlay.path = path.to_string();
    overlay.title = question.trim().to_string();
}

pub fn read_logical_markdown(path: &str) -> Option<String> {
    let host = decl_ui::host_file_from_logical(path);
    std::fs::read_to_string(host).ok()
}

pub fn show_document_overlay(
    ctx: &egui::Context,
    overlay: &mut DocumentOverlayState,
    md_cache: &mut CommonMarkCache,
    t: &UiStrings,
) {
    if !overlay.open {
        return;
    }
    let path = overlay.path.clone();
    let title = overlay.title.clone();
    let body = read_logical_markdown(&path).unwrap_or_else(|| {
        format!("_(could not read {path})_")
    });
    let mut close = false;
    let avail = ctx.available_rect();
    let (default_size, max_size, min_size) =
        overlay_window_sizes(avail.width(), avail.height());
    const FOOTER_H: f32 = 34.0_f32;

    egui::Window::new(&title)
        .collapsible(false)
        .resizable(true)
        .default_size(default_size)
        .min_size(min_size)
        .max_size(max_size)
        .constrain(true)
        .constrain_to(avail)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            let body_h = (ui.available_height() - FOOTER_H).max(80.0_f32);
            egui::ScrollArea::vertical()
                .id_salt(("research_doc_overlay", path.as_str()))
                .auto_shrink([false, false])
                .max_height(body_h)
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width() - 8.0_f32);
                    CommonMarkViewer::new().show(ui, md_cache, &body);
                });
            ui.separator();
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(t.document_overlay_close).clicked() {
                        close = true;
                    }
                });
            });
        });
    if close {
        overlay.open = false;
    }
}

pub fn show_documents_list(
    ctx: &egui::Context,
    list: &mut DocumentsListState,
    entries: &[ResearchDocumentEntry],
    overlay: &mut DocumentOverlayState,
    t: &UiStrings,
) {
    if !list.open {
        return;
    }
    let mut close = false;
    let mut open_path: Option<(String, String)> = None;
    let avail = ctx.available_rect();
    let (default_size, max_size, min_size) =
        overlay_window_sizes(avail.width(), avail.height());
    const FOOTER_H: f32 = 34.0_f32;

    egui::Window::new(t.documents_list_title)
        .collapsible(false)
        .resizable(true)
        .default_size([default_size[0], default_size[1].min(420.0_f32)])
        .min_size(min_size)
        .max_size(max_size)
        .constrain(true)
        .constrain_to(avail)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            let body_h = (ui.available_height() - FOOTER_H).max(80.0_f32);
            egui::ScrollArea::vertical()
                .id_salt("research_documents_list")
                .auto_shrink([false, false])
                .max_height(body_h)
                .show(ui, |ui| {
                    if entries.is_empty() {
                        ui.weak(t.documents_list_empty);
                    } else {
                        for entry in entries {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.strong(entry.question.trim());
                                    ui.weak(&entry.label);
                                });
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui.button(t.document_result_open).clicked() {
                                            open_path = Some((
                                                entry.question.clone(),
                                                entry.path.clone(),
                                            ));
                                        }
                                    },
                                );
                            });
                            ui.separator();
                        }
                    }
                });
            ui.separator();
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(t.document_overlay_close).clicked() {
                        close = true;
                    }
                });
            });
        });
    if close {
        list.open = false;
    }
    if let Some((question, path)) = open_path {
        list.open = false;
        open_document(overlay, &question, &path);
    }
}

pub fn load_index_entries() -> Vec<ResearchDocumentEntry> {
    let home = PathBuf::from(aos_home());
    aos_agent::document_index::load_research_documents(&home)
}

pub fn progress_attachment(question: &str, agent_id: &str) -> ChatAttachment {
    ChatAttachment::DocumentProgress {
        question: question.to_string(),
        agent_id: agent_id.to_string(),
        state: "researching".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_sizes_respect_viewport() {
        let (def, max, min) = overlay_window_sizes(400.0, 300.0);
        assert!(def[0] <= max[0]);
        assert!(def[1] <= max[1]);
        assert!(min[0] <= max[0]);
    }
}
