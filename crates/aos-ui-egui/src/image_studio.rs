//! First-class Image studio page (P09.8) — closed sd.cpp options, no webview.

use crate::cmd::Cmd;
use crate::i18n::UiStrings;
use crate::os_open::aos_home;
use aos_proto::MediaImageOptions;
use eframe::egui;
use std::sync::mpsc::Sender;

pub struct ImageStudioState {
    pub prompt: String,
    pub negative: String,
    pub width: u32,
    pub height: u32,
    pub steps: u32,
    pub cfg: f32,
    pub seed: String,
    pub sampler: String,
    pub style: String,
    pub lora: String,
    pub vae: String,
    pub model_id: String,
    pub preview: Option<String>,
    pub packs: Vec<(String, String)>,
    pub styles: Vec<String>,
    pub loras: Vec<String>,
    pub vaes: Vec<String>,
}

impl Default for ImageStudioState {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            negative: String::new(),
            width: 512,
            height: 512,
            steps: 20,
            cfg: 7.0,
            seed: String::new(),
            sampler: String::new(),
            style: String::new(),
            lora: String::new(),
            vae: String::new(),
            model_id: String::new(),
            preview: None,
            packs: Vec::new(),
            styles: Vec::new(),
            loras: Vec::new(),
            vaes: Vec::new(),
        }
    }
}

impl ImageStudioState {
    pub fn refresh_catalog(&mut self) {
        self.packs.clear();
        self.styles.clear();
        self.loras.clear();
        self.vaes.clear();
        let path = aos_home().join("share/models/catalog-offerings.json");
        let Ok(raw) = std::fs::read_to_string(path) else {
            return;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) else {
            return;
        };
        let Some(models) = v.get("models").and_then(|m| m.as_array()) else {
            return;
        };
        for m in models {
            let id = m.get("id").and_then(|x| x.as_str()).unwrap_or("");
            let name = m.get("name").and_then(|x| x.as_str()).unwrap_or(id);
            let image = m
                .get("profiles")
                .and_then(|p| p.as_array())
                .map(|a| a.iter().any(|x| x.as_str() == Some("image")))
                .unwrap_or(false)
                || m.get("modality").and_then(|x| x.as_str()) == Some("image");
            if image && !id.is_empty() {
                self.packs.push((id.to_string(), name.to_string()));
            }
            if let Some(extras) = m.get("extra_files").and_then(|e| e.as_array()) {
                for f in extras {
                    let fname = f
                        .get("filename")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string();
                    if fname.is_empty() {
                        continue;
                    }
                    match f.get("role").and_then(|x| x.as_str()) {
                        Some("vae") => self.vaes.push(fname),
                        Some("lora") => self.loras.push(fname),
                        Some("style") => self.styles.push(fname),
                        _ => {}
                    }
                }
            }
        }
        if self.model_id.is_empty() {
            if let Some((id, _)) = self.packs.first() {
                self.model_id = id.clone();
            }
        }
    }

    pub fn open_from_chat(&mut self, prompt: &str, path: &str) {
        if !prompt.is_empty() {
            self.prompt = prompt.to_string();
        }
        self.preview = Some(path.to_string());
        self.refresh_catalog();
    }

    pub fn to_options(&self) -> MediaImageOptions {
        MediaImageOptions {
            width: Some(self.width),
            height: Some(self.height),
            steps: Some(self.steps),
            cfg_scale: Some(self.cfg),
            seed: self.seed.parse().ok(),
            sampling_method: if self.sampler.is_empty() {
                None
            } else {
                Some(self.sampler.clone())
            },
            negative_prompt: if self.negative.is_empty() {
                None
            } else {
                Some(self.negative.clone())
            },
            threads: None,
            style: if self.style.is_empty() {
                None
            } else {
                Some(self.style.clone())
            },
            lora: if self.lora.is_empty() {
                None
            } else {
                Some(self.lora.clone())
            },
            vae: if self.vae.is_empty() {
                None
            } else {
                Some(self.vae.clone())
            },
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, t: &UiStrings, cmd: &Sender<Cmd>) {
        if self.packs.is_empty() {
            self.refresh_catalog();
        }
        ui.heading(t.tab_image);
        ui.label(t.tab_hint_image);
        ui.add_space(8.0);
        ui.label("Prompt");
        ui.add(
            egui::TextEdit::multiline(&mut self.prompt)
                .desired_rows(3)
                .desired_width(f32::INFINITY),
        );
        ui.label(t.studio_negative);
        ui.add(
            egui::TextEdit::singleline(&mut self.negative).desired_width(f32::INFINITY),
        );
        ui.horizontal(|ui| {
            ui.label("W");
            ui.add(egui::DragValue::new(&mut self.width).range(64..=2048));
            ui.label("H");
            ui.add(egui::DragValue::new(&mut self.height).range(64..=2048));
            ui.label("steps");
            ui.add(egui::DragValue::new(&mut self.steps).range(1..=150));
            ui.label("CFG");
            ui.add(egui::DragValue::new(&mut self.cfg).range(0.0..=20.0).speed(0.1));
        });
        ui.horizontal(|ui| {
            ui.label("seed");
            ui.add(egui::TextEdit::singleline(&mut self.seed).desired_width(80.0));
            ui.label("sampler");
            egui::ComboBox::from_id_salt("studio_sampler")
                .selected_text(if self.sampler.is_empty() {
                    "default"
                } else {
                    self.sampler.as_str()
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.sampler, String::new(), "default");
                    for s in ["euler", "euler_a", "heun", "dpm2", "lcm", "ddim"] {
                        ui.selectable_value(&mut self.sampler, s.to_string(), s);
                    }
                });
        });
        combo_id(ui, "pack", t.settings_image_pack, &mut self.model_id, &self.packs);
        combo_plain(ui, "style", "Style", &mut self.style, &self.styles);
        combo_plain(ui, "lora", "LoRA", &mut self.lora, &self.loras);
        combo_plain(ui, "vae", "VAE", &mut self.vae, &self.vaes);
        if ui.button(t.studio_generate).clicked() && !self.prompt.is_empty() {
            let _ = cmd.send(Cmd::MediaImage {
                prompt: self.prompt.clone(),
                model_id: if self.model_id.is_empty() {
                    None
                } else {
                    Some(self.model_id.clone())
                },
                options: self.to_options(),
            });
        }
        if let Some(path) = &self.preview {
            ui.label(format!("preview: {path}"));
        }
    }
}

fn combo_id(
    ui: &mut egui::Ui,
    salt: &str,
    label: &str,
    current: &mut String,
    items: &[(String, String)],
) {
    ui.horizontal(|ui| {
        ui.label(label);
        let shown = items
            .iter()
            .find(|(id, _)| id == current)
            .map(|(_, n)| n.as_str())
            .unwrap_or(current.as_str());
        egui::ComboBox::from_id_salt(salt)
            .selected_text(shown)
            .show_ui(ui, |ui| {
                for (id, name) in items {
                    ui.selectable_value(current, id.clone(), name);
                }
            });
    });
}

fn combo_plain(ui: &mut egui::Ui, salt: &str, label: &str, current: &mut String, items: &[String]) {
    ui.horizontal(|ui| {
        ui.label(label);
        let shown = if current.is_empty() {
            "—"
        } else {
            current.as_str()
        };
        egui::ComboBox::from_id_salt(salt)
            .selected_text(shown)
            .show_ui(ui, |ui| {
                ui.selectable_value(current, String::new(), "—");
                for id in items {
                    ui.selectable_value(current, id.clone(), id);
                }
            });
    });
}
