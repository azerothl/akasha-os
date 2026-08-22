//! Premier lancement : confirmation auto-best des modèles (sans bus).

use eframe::egui;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HardwareInfo {
    gpu_name: String,
    vram_mib: u64,
    #[serde(default)]
    tier: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ModelOffering {
    id: String,
    name: String,
    profiles: Vec<String>,
    bytes: u64,
    #[serde(default)]
    optional: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ModelSetupOffer {
    hardware: HardwareInfo,
    recommended_ids: Vec<String>,
    alternative_chat_ids: Vec<String>,
    optional_ids: Vec<String>,
    models: Vec<ModelOffering>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ModelSetupChoice {
    selected_ids: Vec<String>,
    default_chat: String,
    default_embed: String,
    include_optional: bool,
}

pub fn run() -> eframe::Result<()> {
    let home = std::env::var("AOS_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let offer_path = home.join("var/run/model_setup_offer.json");
    let raw = std::fs::read_to_string(&offer_path).unwrap_or_default();
    let offer: ModelSetupOffer = serde_json::from_str(&raw).unwrap_or(ModelSetupOffer {
        hardware: HardwareInfo {
            gpu_name: "unknown".into(),
            vram_mib: 0,
            tier: "mid".into(),
        },
        recommended_ids: vec![],
        alternative_chat_ids: vec![],
        optional_ids: vec![],
        models: vec![],
    });

    let selected: HashSet<String> = offer.recommended_ids.iter().cloned().collect();
    let default_chat = offer
        .recommended_ids
        .iter()
        .find(|id| {
            offer
                .models
                .iter()
                .any(|m| &m.id == *id && m.profiles.iter().any(|p| p == "chat"))
        })
        .cloned()
        .unwrap_or_default();
    let default_embed = offer
        .recommended_ids
        .iter()
        .find(|id| {
            offer
                .models
                .iter()
                .any(|m| &m.id == *id && m.profiles.iter().any(|p| p == "embed"))
        })
        .cloned()
        .unwrap_or_default();
    let include_optional = false;
    let confirmed = false;
    let lang_fr = crate::prefs::detect_os_language() == "fr";

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([640.0, 560.0])
            .with_title("Akasha OS Preview — Models")
            .with_icon(crate::os_open::app_icon()),
        ..Default::default()
    };

    eframe::run_native(
        "Akasha OS Preview — Models",
        options,
        Box::new(move |_cc| {
            Ok(Box::new(SetupApp {
                home,
                offer,
                selected,
                default_chat,
                default_embed,
                include_optional,
                confirmed,
                lang_fr,
            }))
        }),
    )
}

struct SetupApp {
    home: PathBuf,
    offer: ModelSetupOffer,
    selected: HashSet<String>,
    default_chat: String,
    default_embed: String,
    include_optional: bool,
    confirmed: bool,
    lang_fr: bool,
}

impl eframe::App for SetupApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.confirmed {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("model_setup")
                .auto_shrink([false, false])
                .show(ui, |ui| {
            let title = if self.lang_fr {
                "Choix des modèles (auto-best)"
            } else {
                "Model selection (auto-best)"
            };
            ui.heading(title);
            ui.label(format!(
                "GPU: {} — {} MiB VRAM — tier {}",
                self.offer.hardware.gpu_name,
                self.offer.hardware.vram_mib,
                self.offer.hardware.tier
            ));
            ui.separator();
            ui.horizontal(|ui| {
                ui.radio_value(&mut self.lang_fr, false, "English");
                ui.radio_value(&mut self.lang_fr, true, "Français");
            });
            ui.separator();

            let rec_label = if self.lang_fr {
                "Recommandé pour votre machine"
            } else {
                "Recommended for your hardware"
            };
            ui.strong(rec_label);
            for id in &self.offer.recommended_ids {
                if let Some(m) = self.offer.models.iter().find(|x| x.id == *id) {
                    let mut on = self.selected.contains(id);
                    if ui
                        .checkbox(
                            &mut on,
                            format!(
                                "{} ({:.1} GiB) — {:?}",
                                m.name,
                                m.bytes as f64 / (1 << 30) as f64,
                                m.profiles
                            ),
                        )
                        .changed()
                    {
                        if on {
                            self.selected.insert(id.clone());
                        } else {
                            self.selected.remove(id);
                        }
                    }
                }
            }

            ui.add_space(8.0);
            let alt = if self.lang_fr {
                "Alternatives chat"
            } else {
                "Chat alternatives"
            };
            ui.strong(alt);
            for id in &self.offer.alternative_chat_ids {
                if self.offer.recommended_ids.contains(id) {
                    continue;
                }
                if let Some(m) = self.offer.models.iter().find(|x| x.id == *id) {
                    let mut on = self.selected.contains(id);
                    if ui
                        .checkbox(
                            &mut on,
                            format!(
                                "{} ({:.1} GiB)",
                                m.name,
                                m.bytes as f64 / (1 << 30) as f64
                            ),
                        )
                        .changed()
                    {
                        if on {
                            self.selected.insert(id.clone());
                            self.default_chat = id.clone();
                        } else {
                            self.selected.remove(id);
                        }
                    }
                    if self.selected.contains(id) {
                        ui.radio_value(&mut self.default_chat, id.clone(), "default chat");
                    }
                }
            }

            ui.add_space(8.0);
            let opt_lab = if self.lang_fr {
                "Télécharger aussi les modèles optionnels (plus lourds)"
            } else {
                "Also download optional larger models"
            };
            ui.checkbox(&mut self.include_optional, opt_lab);
            if self.include_optional {
                for id in &self.offer.optional_ids {
                    if let Some(m) = self.offer.models.iter().find(|x| x.id == *id) {
                        let mut on = self.selected.contains(id);
                        if ui
                            .checkbox(
                                &mut on,
                                format!(
                                    "{} ({:.1} GiB) {:?}",
                                    m.name,
                                    m.bytes as f64 / (1 << 30) as f64,
                                    m.profiles
                                ),
                            )
                            .changed()
                        {
                            if on {
                                self.selected.insert(id.clone());
                            } else {
                                self.selected.remove(id);
                            }
                        }
                    }
                }
            }

            ui.separator();
            let total: u64 = self
                .offer
                .models
                .iter()
                .filter(|m| self.selected.contains(&m.id))
                .map(|m| m.bytes)
                .sum();
            ui.label(format!(
                "{}: {:.1} GiB",
                if self.lang_fr {
                    "Téléchargement estimé"
                } else {
                    "Estimated download"
                },
                total as f64 / (1 << 30) as f64
            ));

            ui.horizontal(|ui| {
                let ok = if self.lang_fr {
                    "Télécharger et continuer"
                } else {
                    "Download and continue"
                };
                if ui.button(ok).clicked() {
                    // Ensure defaults are selected.
                    self.selected.insert(self.default_embed.clone());
                    self.selected.insert(self.default_chat.clone());
                    let choice = ModelSetupChoice {
                        selected_ids: self.selected.iter().cloned().collect(),
                        default_chat: self.default_chat.clone(),
                        default_embed: self.default_embed.clone(),
                        include_optional: self.include_optional,
                    };
                    let dir = self.home.join("var/models");
                    let _ = std::fs::create_dir_all(&dir);
                    if let Ok(s) = serde_json::to_string_pretty(&choice) {
                        let _ = std::fs::write(dir.join("setup_choice.json"), s);
                    }
                    self.confirmed = true;
                }
                let cancel = if self.lang_fr { "Annuler" } else { "Cancel" };
                if ui.button(cancel).clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            });
        });
    }
}
