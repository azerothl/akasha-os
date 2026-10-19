//! Native Dev Assistant surfaces. All data comes from cap-checked module tools;
//! these widgets never read files, execute code, or apply patches themselves.
use aos_proto::decl_ui::DeclUiWidget;
use eframe::egui::{self, Ui};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

pub(super) fn render(
    ui: &mut Ui,
    w: &DeclUiWidget,
    value: &Value,
    local: &HashMap<String, Value>,
    fields: &mut HashMap<String, String>,
    enabled: bool,
    language: &str,
) -> HashMap<String, Value> {
    let mut patch = HashMap::new();
    let french = language.starts_with("fr");
    match w.kind.as_str() {
        "code_editor" => {
            let key = w
                .state_key
                .as_deref()
                .or(w.label.as_deref())
                .unwrap_or("content");
            let initial = value
                .as_str()
                .or_else(|| value.get("content").and_then(Value::as_str))
                .unwrap_or("");
            let mut text = if w.read_only.unwrap_or(false) {
                initial.to_string()
            } else if w.state_key.is_some() {
                local
                    .get(key)
                    .and_then(Value::as_str)
                    .unwrap_or(initial)
                    .to_string()
            } else {
                fields
                    .entry(key.into())
                    .or_insert_with(|| initial.into())
                    .clone()
            };
            if text.len() > 65_536 {
                ui.label(if french {
                    "Fichier trop grand pour cet éditeur (64 Kio)."
                } else {
                    "File exceeds this editor's 64 KiB limit."
                });
                return patch;
            }
            let readonly = w.read_only.unwrap_or(false);
            ui.add_enabled_ui(enabled && !readonly, |ui| {
                egui::ScrollArea::both()
                    .id_salt(("code", key))
                    .max_height(w.size.unwrap_or(300.0).clamp(100.0, 600.0))
                    .show(ui, |ui| {
                        let response = ui.add(
                            egui::TextEdit::multiline(&mut text)
                                .code_editor()
                                .desired_rows(12)
                                .desired_width(f32::INFINITY),
                        );
                        if response.changed() && text.len() <= 65_536 {
                            if w.state_key.is_some() {
                                patch.insert(key.into(), Value::String(text.clone()));
                            } else {
                                fields.insert(key.into(), text.clone());
                            }
                        }
                    });
            });
            ui.small(if readonly {
                if french {
                    "Lecture seule. Relisez le fichier pour actualiser cet aperçu."
                } else {
                    "Read only. Read the file again to refresh this preview."
                }
            } else if french {
                "Les modifications sont locales jusqu'à l'action Appliquer."
            } else {
                "Changes stay local until Apply is invoked."
            });
        }
        "diff_view" => {
            let text = value
                .as_str()
                .or_else(|| value.get("diff").and_then(Value::as_str))
                .unwrap_or("");
            egui::ScrollArea::both()
                .id_salt(("diff", w.bind.as_deref()))
                .max_height(360.0)
                .show(ui, |ui| {
                    for line in text.lines().take(5000) {
                        let color = if line.starts_with('+') && !line.starts_with("+++") {
                            ui.visuals().hyperlink_color
                        } else if line.starts_with('-') && !line.starts_with("---") {
                            ui.visuals().error_fg_color
                        } else {
                            ui.visuals().text_color()
                        };
                        ui.label(egui::RichText::new(line).monospace().color(color));
                    }
                    if text.lines().count() > 5000
                        || value.get("truncated") == Some(&Value::Bool(true))
                    {
                        ui.label(if french {
                            "Diff partiel : limite d'affichage atteinte."
                        } else {
                            "Partial diff: display limit reached."
                        });
                    }
                });
        }
        "file_tree" => {
            let rows = value
                .as_array()
                .or_else(|| value.get("entries").and_then(Value::as_array));
            let mut tree = Tree::default();
            if let Some(rows) = rows {
                for row in rows.iter().take(2000) {
                    if let Some(path) = row.get("path").and_then(Value::as_str) {
                        tree.insert(path);
                    }
                }
            }
            let selected = w.selected_key.as_deref().unwrap_or("selected_path");
            let current = local.get(selected).and_then(Value::as_str).unwrap_or("");
            egui::ScrollArea::vertical()
                .id_salt(("files", w.bind.as_deref()))
                .max_height(360.0)
                .show(ui, |ui| {
                    ui.add_enabled_ui(enabled, |ui| {
                        if let Some(path) = tree.paint(ui, current) {
                            patch.insert(selected.into(), Value::String(path));
                        }
                    });
                });
        }
        "problems_list" => {
            let rows = value
                .as_array()
                .or_else(|| value.get("problems").and_then(Value::as_array));
            egui::ScrollArea::vertical()
                .id_salt(("problems", w.bind.as_deref()))
                .max_height(260.0)
                .show(ui, |ui| {
                    if let Some(rows) = rows {
                        for problem in rows.iter().take(500) {
                            let path = problem["path"].as_str().unwrap_or("");
                            let label = format!(
                                "{}:{}:{} — {}",
                                path,
                                problem["line"].as_u64().unwrap_or(1),
                                problem["column"].as_u64().unwrap_or(1),
                                problem["message"].as_str().unwrap_or("")
                            );
                            let text = egui::RichText::new(label).color(
                                if problem["severity"] == "error" {
                                    ui.visuals().error_fg_color
                                } else {
                                    ui.visuals().text_color()
                                },
                            );
                            if ui
                                .add_enabled(enabled, egui::Button::new(text).frame(false))
                                .clicked()
                            {
                                patch.insert(
                                    w.selected_key.clone().unwrap_or("selected_path".into()),
                                    Value::String(path.into()),
                                );
                            }
                        }
                    }
                });
        }
        _ => {}
    }
    patch
}

#[derive(Default)]
struct Tree {
    path: Option<String>,
    children: BTreeMap<String, Tree>,
}
impl Tree {
    fn insert(&mut self, path: &str) {
        let parts: Vec<_> = path
            .split('/')
            .filter(|p| !p.is_empty() && *p != "." && *p != "..")
            .take(32)
            .collect();
        let mut node = self;
        for part in parts {
            node = node.children.entry(part.into()).or_default();
        }
        node.path = Some(path.into());
    }
    fn paint(&self, ui: &mut Ui, selected: &str) -> Option<String> {
        let mut choice = None;
        for (label, node) in &self.children {
            if node.children.is_empty() {
                if let Some(path) = &node.path {
                    if ui.selectable_label(path == selected, label).clicked() {
                        choice = Some(path.clone());
                    }
                }
            } else {
                let result = egui::CollapsingHeader::new(label)
                    .id_salt(("dev-tree", label, node.path.as_deref()))
                    .show(ui, |ui| node.paint(ui, selected));
                if let Some(Some(path)) = result.body_returned {
                    choice = Some(path);
                }
            }
        }
        choice
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tree_groups_paths_without_touching_the_filesystem() {
        let mut tree = Tree::default();
        tree.insert("/host/repo/src/main.rs");
        tree.insert("/host/repo/src/lib.rs");
        assert_eq!(
            tree.children["host"].children["repo"].children["src"]
                .children
                .len(),
            2
        );
    }
    #[test]
    fn dev_surfaces_render_headlessly_and_do_not_invoke_tools() {
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                for kind in ["code_editor", "diff_view", "file_tree", "problems_list"] {
                    let w: DeclUiWidget = serde_json::from_value(
                        serde_json::json!({"kind":kind,"label":"content","bind":"fixture"}),
                    )
                    .unwrap();
                    let mut fields = HashMap::new();
                    assert!(render(
                        ui,
                        &w,
                        &Value::String("+example\n-old".into()),
                        &HashMap::new(),
                        &mut fields,
                        true,
                        "fr"
                    )
                    .is_empty());
                }
            });
        });
    }
    #[test]
    fn readonly_source_preview_uses_latest_tool_response_not_stale_form_text() {
        let ctx = egui::Context::default();
        let mut fields = HashMap::from([("preview".into(), "stale_source_value".into())]);
        let w: DeclUiWidget = serde_json::from_value(
            serde_json::json!({"kind":"code_editor","label":"preview","read_only":true}),
        )
        .unwrap();
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                render(
                    ui,
                    &w,
                    &Value::String("fresh_source_value".into()),
                    &HashMap::new(),
                    &mut fields,
                    true,
                    "fr",
                );
            });
        });
        fn texts(shape: &egui::epaint::Shape, out: &mut Vec<String>) {
            match shape {
                egui::epaint::Shape::Text(text) => out.push(text.galley.job.text.clone()),
                egui::epaint::Shape::Vec(children) => {
                    for child in children {
                        texts(child, out)
                    }
                }
                _ => {}
            }
        }
        let mut rendered = Vec::new();
        for shape in output.shapes {
            texts(&shape.shape, &mut rendered);
        }
        assert!(rendered
            .iter()
            .any(|text| text.contains("fresh_source_value")));
        assert!(!rendered
            .iter()
            .any(|text| text.contains("stale_source_value")));
    }
}
