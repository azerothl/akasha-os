//! Clickable artifact cards in chat bubbles (note / document / image).

#[cfg(test)]
use aos_proto::ChatAttachment;
use eframe::egui;

use crate::cmd::Cmd;
use crate::decl_ui;
use crate::i18n::UiStrings;
use crate::icons;
use crate::os_open::{aos_home, open_os_folder};
use crate::research_document;
use crate::{Tab, UiApp};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactKind {
    Note,
    Document,
    Image,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactTarget {
    pub title: String,
    pub path: String,
    pub kind: ArtifactKind,
    pub slug: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactCardAction {
    None,
    Open(ArtifactTarget),
}

pub fn parse_attachment(
    title: &str,
    artifact_type: &str,
    path: &str,
    slug: &str,
) -> Option<ArtifactTarget> {
    let kind = match artifact_type.trim() {
        "note" => ArtifactKind::Note,
        "document" => ArtifactKind::Document,
        "image" => ArtifactKind::Image,
        _ => return None,
    };
    let path = path.trim();
    if path.is_empty() {
        return None;
    }
    Some(ArtifactTarget {
        title: title.trim().to_string(),
        path: path.to_string(),
        kind,
        slug: slug.trim().to_string(),
    })
}

pub fn type_label(t: &UiStrings, kind: ArtifactKind) -> &'static str {
    match kind {
        ArtifactKind::Note => t.artifact_type_note,
        ArtifactKind::Document => t.artifact_type_document,
        ArtifactKind::Image => t.artifact_type_image,
    }
}

fn short_artifact_path(path: &str) -> String {
    let norm = path.replace('\\', "/");
    let parts: Vec<&str> = norm.split('/').filter(|s| !s.is_empty()).collect();
    if parts.len() <= 2 {
        parts.join("/")
    } else {
        format!("{}/{}", parts[parts.len() - 2], parts[parts.len() - 1])
    }
}

pub fn render_artifact_card(
    ui: &mut egui::Ui,
    t: &UiStrings,
    target: &ArtifactTarget,
) -> ArtifactCardAction {
    let mut action = ArtifactCardAction::None;
    let type_label = type_label(t, target.kind);
    let accent = crate::theme::button_colors(ui).accent;
    let glyph = match target.kind {
        ArtifactKind::Note => icons::OverflowNavIcon::Notes,
        ArtifactKind::Document => icons::OverflowNavIcon::Documents,
        ArtifactKind::Image => icons::OverflowNavIcon::Files,
    };
    let max_w = ui.available_width().min(340.0);
    let card = egui::Frame::NONE
        .fill(ui.visuals().faint_bg_color)
        .stroke(egui::Stroke::new(
            1.0_f32,
            ui.visuals().widgets.noninteractive.bg_stroke.color,
        ))
        .inner_margin(egui::Margin::symmetric(12, 8))
        .corner_radius(crate::theme::RADIUS_MD)
        .show(ui, |ui| {
            ui.set_min_width(max_w.min(280.0));
            ui.set_max_width(max_w);
            ui.horizontal(|ui| {
                icons::overflow_glyph(ui, glyph, accent, 18.0);
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.colored_label(
                        accent,
                        egui::RichText::new(type_label.to_uppercase())
                            .small()
                            .strong(),
                    );
                    let title = if target.title.is_empty() {
                        type_label
                    } else {
                        target.title.as_str()
                    };
                    ui.label(egui::RichText::new(title).strong());
                    let path = short_artifact_path(&target.path);
                    if !path.is_empty() {
                        ui.weak(egui::RichText::new(path).small());
                    }
                });
            });
        });
    if card.response.interact(egui::Sense::click()).clicked() {
        action = ArtifactCardAction::Open(target.clone());
    }
    action
}

#[allow(dead_code)]
pub fn card_display_text(t: &UiStrings, target: &ArtifactTarget) -> String {
    format!("{} — {}", target.title, type_label(t, target.kind))
}

/// Open the dedicated surface when possible; returns false when folder fallback is needed.
pub fn open_artifact_target(app: &mut UiApp, target: &ArtifactTarget) -> bool {
    match target.kind {
        ArtifactKind::Note => open_note(app, target),
        ArtifactKind::Document => open_document(target),
        ArtifactKind::Image => open_image(app, target),
    }
}

fn open_note(app: &mut UiApp, target: &ArtifactTarget) -> bool {
    let _ = app.cmd_tx.send(Cmd::NotesList);
    let title = if target.title.is_empty() {
        None
    } else {
        Some(target.title.clone())
    };
    let slug = if target.slug.is_empty() {
        None
    } else {
        Some(target.slug.clone())
    };
    let _ = app.cmd_tx.send(Cmd::NotesRead {
        title,
        path: Some(target.path.clone()),
        slug,
    });
    app.tab = Tab::Notes;
    true
}

fn open_document(target: &ArtifactTarget) -> bool {
    research_document::read_logical_markdown(&target.path).is_some()
}

fn open_image(app: &mut UiApp, target: &ArtifactTarget) -> bool {
    if decl_ui::host_file_from_logical(&target.path).exists() {
        crate::create_nav::open_create_module_if_installed(
            app,
            Some(&target.title),
            Some(&target.path),
        )
    } else {
        false
    }
}

pub fn open_folder_for_path(path: &str) {
    let host = decl_ui::host_file_from_logical(path);
    let dir = host
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| aos_home().join("var/storage/data"));
    open_os_folder(&dir);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locked_type_labels_en_fr() {
        let en = crate::i18n::strings("en");
        let fr = crate::i18n::strings("fr");
        assert_eq!(en.artifact_type_note, "Note");
        assert_eq!(fr.artifact_type_note, "Note");
        assert_eq!(en.artifact_type_document, "Document");
        assert_eq!(fr.artifact_type_document, "Document");
        assert_eq!(en.artifact_type_image, "Image");
        assert_eq!(fr.artifact_type_image, "Image");
        assert_eq!(en.artifact_open_folder, "Open folder");
        assert_eq!(fr.artifact_open_folder, "Ouvrir le dossier");
    }

    #[test]
    fn parse_attachment_maps_kind() {
        let target = parse_attachment(
            "Cohort plan",
            "note",
            "/documents/notes/cohort.md",
            "cohort",
        )
        .unwrap();
        assert_eq!(target.kind, ArtifactKind::Note);
        assert_eq!(target.slug, "cohort");
    }

    #[test]
    fn card_display_has_no_path_or_tool_id() {
        let t = crate::i18n::strings("en");
        let target = ArtifactTarget {
            title: "Report".into(),
            path: "/downloads/report.md".into(),
            kind: ArtifactKind::Document,
            slug: String::new(),
        };
        let label = card_display_text(&t, &target);
        assert!(!label.contains('/'));
        assert!(!label.contains("files.generate"));
        assert!(!label.contains("notes."));
        assert!(label.contains("Report"));
        assert!(label.contains("Document"));
    }

    #[test]
    fn attachment_round_trip() {
        let att = ChatAttachment::ArtifactCard {
            title: "Fox".into(),
            artifact_type: "image".into(),
            path: "/downloads/fox.png".into(),
            slug: String::new(),
        };
        match att {
            ChatAttachment::ArtifactCard {
                title,
                artifact_type,
                path,
                slug,
            } => {
                let target = parse_attachment(&title, &artifact_type, &path, &slug).unwrap();
                assert_eq!(target.kind, ArtifactKind::Image);
                assert_eq!(target.path, "/downloads/fox.png");
            }
            _ => panic!("expected ArtifactCard"),
        }
    }
}
