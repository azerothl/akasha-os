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

/// Square window chrome for document overlay + list (not rounded slide/card).
pub(crate) fn square_document_window_frame(style: &egui::Style) -> egui::Frame {
    egui::Frame::window(style).corner_radius(egui::CornerRadius::ZERO)
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
    let body = read_logical_markdown(&path).unwrap_or_else(|| t.document_open_failed.to_string());
    let mut close = false;
    let avail = ctx.available_rect();
    let (default_size, max_size, min_size) =
        overlay_window_sizes(avail.width(), avail.height());
    const FOOTER_H: f32 = 34.0_f32;

    egui::Window::new(&title)
        .collapsible(false)
        .resizable(true)
        .frame(square_document_window_frame(&ctx.style()))
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
                    // Prepared research output is a markdown document (CommonMark), not a slide deck.
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
        .frame(square_document_window_frame(&ctx.style()))
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
                                    if let Some(date) = format_entry_date(entry.created_ms) {
                                        ui.weak(date);
                                    }
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

fn format_entry_date(ts_ms: u64) -> Option<String> {
    if ts_ms == 0 {
        return None;
    }
    let secs = (ts_ms / 1000) as i64;
    let days = secs.div_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    Some(format!("{year:04}-{month:02}-{day:02}"))
}

fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let y = y + if m <= 2 { 1 } else { 0 };
    (y as i32, m as u32, d as u32)
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
    fn document_window_frame_is_square() {
        let style = egui::Style::default();
        let frame = square_document_window_frame(&style);
        assert_eq!(frame.corner_radius, egui::CornerRadius::ZERO);
    }

    #[test]
    fn open_document_title_is_question_not_path() {
        let mut overlay = DocumentOverlayState::default();
        open_document(
            &mut overlay,
            "What is agentic?",
            "/downloads/research-agentic.md",
        );
        assert_eq!(overlay.title, "What is agentic?");
        assert!(!overlay.title.contains(".md"));
    }

    #[test]
    fn overlay_sizes_respect_viewport() {
        let (def, max, min) = overlay_window_sizes(400.0, 300.0);
        assert!(def[0] <= max[0]);
        assert!(def[1] <= max[1]);
        assert!(min[0] <= max[0]);
    }

    #[test]
    fn locked_open_failure_copy() {
        let t_en = crate::i18n::strings("en");
        let t_fr = crate::i18n::strings("fr");
        assert_eq!(t_en.document_open_failed, "Couldn't open this document.");
        assert_eq!(
            t_fr.document_open_failed,
            "Impossible d'ouvrir ce document."
        );
        assert!(!t_en.document_open_failed.contains('/'));
        assert!(!t_fr.document_open_failed.contains('/'));
    }

    #[test]
    fn entry_date_never_uses_filename() {
        let date = format_entry_date(1_725_000_000_000).expect("date");
        assert!(date.contains('-'));
        assert!(!date.contains(".md"));
        assert!(!date.contains('/'));
    }
}
