//! First-run model selection (auto-best offer + optional remote providers).

use aos_model::RemoteOpenAiBackend;
use aos_proto::ProviderRecord;
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

fn offering_has_profile(offer: &ModelSetupOffer, id: &str, profile: &str) -> bool {
    offer
        .models
        .iter()
        .find(|m| m.id == id)
        .is_some_and(|m| m.profiles.iter().any(|p| p == profile))
}

fn is_remote_model_id(id: &str) -> bool {
    id.starts_with("provider:") || id.starts_with("remote:")
}

fn remote_model_id(provider_id: &str, model: &str) -> String {
    format!("provider:{provider_id}:{model}")
}

fn selection_valid(
    offer: &ModelSetupOffer,
    selected: &HashSet<String>,
    default_chat: &str,
    default_embed: &str,
) -> bool {
    let chat_ok = if is_remote_model_id(default_chat) {
        !default_chat.trim().is_empty()
    } else if !default_chat.is_empty() && selected.contains(default_chat) {
        offering_has_profile(offer, default_chat, "chat")
    } else {
        selected
            .iter()
            .any(|id| offering_has_profile(offer, id, "chat"))
    };
    let embed_ok = if is_remote_model_id(default_embed) {
        !default_embed.trim().is_empty()
    } else if !default_embed.is_empty() && selected.contains(default_embed) {
        offering_has_profile(offer, default_embed, "embed")
    } else {
        selected
            .iter()
            .any(|id| offering_has_profile(offer, id, "embed"))
    };
    chat_ok && embed_ok
}

fn resolve_defaults(
    offer: &ModelSetupOffer,
    selected: &HashSet<String>,
    default_chat: &str,
    default_embed: &str,
) -> (String, String) {
    let chat = if is_remote_model_id(default_chat) {
        default_chat.to_string()
    } else if !default_chat.is_empty() && selected.contains(default_chat) {
        default_chat.to_string()
    } else {
        selected
            .iter()
            .find(|id| offering_has_profile(offer, id, "chat"))
            .cloned()
            .unwrap_or_default()
    };
    let embed = if is_remote_model_id(default_embed) {
        default_embed.to_string()
    } else if !default_embed.is_empty() && selected.contains(default_embed) {
        default_embed.to_string()
    } else {
        selected
            .iter()
            .find(|id| offering_has_profile(offer, id, "embed"))
            .cloned()
            .unwrap_or_default()
    };
    (chat, embed)
}

fn save_provider_secret(home: &PathBuf, secret_name: &str, value: &str) -> Result<(), String> {
    if secret_name.trim().is_empty() || value.is_empty() {
        return Ok(());
    }
    let secrets_dir = home.join("var/secrets");
    let mut store =
        aos_platform::SecretStore::open(&secrets_dir).map_err(|e| format!("vault: {e}"))?;
    store
        .set(secret_name, value, "ui-egui")
        .map_err(|e| format!("vault: {e}"))
}

fn test_provider(
    provider: &ProviderRecord,
    secret_value: Option<&str>,
) -> Result<Vec<String>, String> {
    let key = if let Some(v) = secret_value.filter(|s| !s.is_empty()) {
        Some(v.to_string())
    } else if let Some(name) = provider.secret_name.as_deref().filter(|s| !s.is_empty()) {
        let secrets_dir = std::env::var("AOS_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("var/secrets");
        aos_platform::SecretStore::open(&secrets_dir)
            .ok()
            .and_then(|store| store.get(name, "ui-egui").ok())
    } else {
        None
    };
    let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    rt.block_on(async {
        let be = RemoteOpenAiBackend::new(&provider.endpoint, "probe", key);
        let models = be.list_models().await.unwrap_or_default();
        let ok = be.health().await || !models.is_empty();
        if ok {
            Ok(models)
        } else {
            Err("endpoint unreachable".into())
        }
    })
}

fn collect_remote_model_ids(providers: &[ProviderRecord]) -> Vec<String> {
    let mut out = Vec::new();
    for p in providers {
        if !p.enabled {
            continue;
        }
        for m in &p.discovered_models {
            out.push(remote_model_id(&p.id, m));
        }
    }
    out.sort();
    out.dedup();
    out
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
        .find(|id| offering_has_profile(&offer, id, "chat"))
        .cloned()
        .unwrap_or_default();
    let default_embed = offer
        .recommended_ids
        .iter()
        .find(|id| offering_has_profile(&offer, id, "embed"))
        .cloned()
        .unwrap_or_default();
    let language = crate::prefs::detect_os_language();
    let providers = aos_model::providers::load_all();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([720.0, 680.0])
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
                include_optional: false,
                confirmed: false,
                language,
                validation_error: None,
                providers,
                provider_id: String::new(),
                provider_preset: "openai".into(),
                provider_endpoint: "https://api.openai.com/v1".into(),
                provider_secret_name: "openai_api_key".into(),
                provider_secret_value: String::new(),
                provider_enabled: true,
                provider_status: String::new(),
                provider_testing: false,
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
    language: String,
    validation_error: Option<String>,
    providers: Vec<ProviderRecord>,
    provider_id: String,
    provider_preset: String,
    provider_endpoint: String,
    provider_secret_name: String,
    provider_secret_value: String,
    provider_enabled: bool,
    provider_status: String,
    provider_testing: bool,
}

impl SetupApp {
    fn strings(&self) -> crate::i18n::UiStrings {
        crate::i18n::strings(&self.language)
    }

    fn reload_providers(&mut self) {
        self.providers = aos_model::providers::load_all();
    }

    fn apply_provider_preset(&mut self, name: &str, endpoint: &str, secret: Option<&str>) {
        self.provider_preset = name.into();
        if self.provider_id.is_empty() {
            self.provider_id = name.into();
        }
        if !endpoint.is_empty() {
            self.provider_endpoint = endpoint.into();
        }
        if let Some(s) = secret {
            self.provider_secret_name = s.into();
        }
    }

    fn save_provider(&mut self) -> Result<(), String> {
        let id = self.provider_id.trim().to_string();
        if id.is_empty() {
            return Err("id required".into());
        }
        let provider = ProviderRecord {
            id: id.clone(),
            preset: self.provider_preset.clone(),
            endpoint: self.provider_endpoint.trim().to_string(),
            secret_name: if self.provider_secret_name.trim().is_empty() {
                None
            } else {
                Some(self.provider_secret_name.trim().to_string())
            },
            enabled: self.provider_enabled,
            discovered_models: self
                .providers
                .iter()
                .find(|p| p.id == id)
                .map(|p| p.discovered_models.clone())
                .unwrap_or_default(),
        };
        if let Some(name) = provider.secret_name.as_deref() {
            save_provider_secret(&self.home, name, &self.provider_secret_value)?;
        }
        aos_model::providers::save(&provider)?;
        self.reload_providers();
        Ok(())
    }

    fn test_saved_provider(&mut self, t: &crate::i18n::UiStrings) {
        let id = self.provider_id.trim().to_string();
        if id.is_empty() {
            return;
        }
        self.provider_testing = true;
        let secret = self.provider_secret_value.clone();
        let Some(provider) = self.providers.iter().find(|p| p.id == id).cloned() else {
            self.provider_testing = false;
            self.provider_status = t.model_setup_remote_test_first.to_string();
            return;
        };
        match test_provider(&provider, Some(&secret)) {
            Ok(models) => {
                let mut rec = provider;
                if !models.is_empty() {
                    rec.discovered_models = models.clone();
                    let _ = aos_model::providers::save(&rec);
                    self.reload_providers();
                }
                self.provider_status = t
                    .model_setup_provider_test_ok
                    .replace("{}", &models.len().to_string());
            }
            Err(e) => {
                self.provider_status = t.model_setup_provider_test_fail.replace("{}", &e);
            }
        }
        self.provider_testing = false;
    }

    fn ui_providers(&mut self, ui: &mut egui::Ui, t: &crate::i18n::UiStrings) {
        ui.collapsing(t.model_setup_remote_section, |ui| {
            ui.weak(t.model_setup_remote_hint);
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("id");
                ui.text_edit_singleline(&mut self.provider_id);
            });
            ui.horizontal(|ui| {
                ui.label(t.providers_preset);
                egui::ComboBox::from_id_salt("model_setup_provider_preset")
                    .selected_text(&self.provider_preset)
                    .show_ui(ui, |ui| {
                        for &(name, endpoint, secret) in aos_proto::PROVIDER_PRESETS {
                            if ui
                                .selectable_label(self.provider_preset == name, name)
                                .clicked()
                            {
                                self.apply_provider_preset(name, endpoint, secret);
                            }
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.label(t.providers_endpoint);
                ui.add(
                    egui::TextEdit::singleline(&mut self.provider_endpoint).desired_width(420.0),
                );
            });
            ui.horizontal(|ui| {
                ui.label(t.providers_secret);
                ui.text_edit_singleline(&mut self.provider_secret_name);
            });
            ui.horizontal(|ui| {
                ui.label("API key (vault)");
                ui.add(
                    egui::TextEdit::singleline(&mut self.provider_secret_value)
                        .password(true)
                        .desired_width(280.0),
                );
            });
            ui.checkbox(&mut self.provider_enabled, t.providers_enabled);
            ui.horizontal(|ui| {
                if ui.button(t.providers_save).clicked() {
                    match self.save_provider() {
                        Ok(()) => self.provider_status = t.model_setup_provider_saved.to_string(),
                        Err(e) => self.provider_status = e,
                    }
                }
                let test = ui.add_enabled(!self.provider_testing, egui::Button::new(t.providers_test));
                if test.clicked() {
                    if self.save_provider().is_ok() {
                        self.test_saved_provider(t);
                    }
                }
            });
            if !self.provider_status.is_empty() {
                ui.colored_label(egui::Color32::LIGHT_GRAY, &self.provider_status);
            }
            ui.add_space(6.0);
            ui.strong(t.model_setup_remote_models);
            let remote_ids = collect_remote_model_ids(&self.providers);
            if remote_ids.is_empty() {
                ui.weak(t.model_setup_remote_test_first);
            } else {
                for id in remote_ids {
                    ui.horizontal(|ui| {
                        ui.label(&id);
                        ui.radio_value(&mut self.default_chat, id.clone(), t.model_setup_default_chat);
                        ui.radio_value(
                            &mut self.default_embed,
                            id.clone(),
                            t.model_setup_default_embed,
                        );
                    });
                }
            }
        });
    }

    fn ui_local_defaults(&mut self, ui: &mut egui::Ui, t: &crate::i18n::UiStrings) {
        let embed_ids: Vec<String> = self
            .selected
            .iter()
            .filter(|id| offering_has_profile(&self.offer, id, "embed"))
            .cloned()
            .collect();
        if embed_ids.len() > 1 {
            ui.strong(t.model_setup_default_embed);
            for id in embed_ids {
                if let Some(m) = self.offer.models.iter().find(|x| x.id == id) {
                    ui.radio_value(&mut self.default_embed, id.clone(), &m.name);
                }
            }
        }
        let chat_ids: Vec<String> = self
            .selected
            .iter()
            .filter(|id| offering_has_profile(&self.offer, id, "chat"))
            .cloned()
            .collect();
        if chat_ids.len() > 1 {
            ui.strong(t.model_setup_default_chat);
            for id in chat_ids {
                if let Some(m) = self.offer.models.iter().find(|x| x.id == id) {
                    ui.radio_value(&mut self.default_chat, id.clone(), &m.name);
                }
            }
        }
    }

    fn try_confirm(&mut self, t: &crate::i18n::UiStrings) {
        if !selection_valid(
            &self.offer,
            &self.selected,
            &self.default_chat,
            &self.default_embed,
        ) {
            self.validation_error = Some(t.model_setup_validation_need_both.to_string());
            return;
        }
        let (default_chat, default_embed) = resolve_defaults(
            &self.offer,
            &self.selected,
            &self.default_chat,
            &self.default_embed,
        );
        if default_chat.is_empty() || default_embed.is_empty() {
            self.validation_error = Some(t.model_setup_validation_need_both.to_string());
            return;
        }
        self.validation_error = None;
        let mut selected_ids: Vec<String> = self
            .selected
            .iter()
            .filter(|id| !is_remote_model_id(id))
            .cloned()
            .collect();
        if !is_remote_model_id(&default_chat) && !selected_ids.contains(&default_chat) {
            selected_ids.push(default_chat.clone());
        }
        if !is_remote_model_id(&default_embed) && !selected_ids.contains(&default_embed) {
            selected_ids.push(default_embed.clone());
        }
        selected_ids.sort();
        selected_ids.dedup();
        let choice = ModelSetupChoice {
            selected_ids,
            default_chat,
            default_embed,
            include_optional: self.include_optional,
        };
        let dir = self.home.join("var/models");
        let _ = std::fs::create_dir_all(&dir);
        if let Ok(s) = serde_json::to_string_pretty(&choice) {
            let _ = std::fs::write(dir.join("setup_choice.json"), s);
        }
        self.confirmed = true;
    }
}

impl eframe::App for SetupApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.confirmed {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        let t = self.strings();
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("model_setup")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.heading(t.model_setup_title);
                    ui.label(format!(
                        "GPU: {} — {} MiB VRAM — tier {}",
                        self.offer.hardware.gpu_name,
                        self.offer.hardware.vram_mib,
                        self.offer.hardware.tier
                    ));
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut self.language, "en".into(), "English");
                        ui.radio_value(&mut self.language, "fr".into(), "Français");
                    });
                    ui.separator();

                    ui.strong(t.model_setup_recommended);
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
                    ui.strong(t.model_setup_chat_alternatives);
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
                                ui.radio_value(
                                    &mut self.default_chat,
                                    id.clone(),
                                    t.model_setup_default_chat,
                                );
                            }
                        }
                    }

                    ui.add_space(8.0);
                    ui.checkbox(&mut self.include_optional, t.model_setup_optional);
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

                    self.ui_local_defaults(ui, &t);
                    ui.separator();
                    self.ui_providers(ui, &t);

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
                        t.model_setup_estimated_download,
                        total as f64 / (1 << 30) as f64
                    ));

                    let can_continue = selection_valid(
                        &self.offer,
                        &self.selected,
                        &self.default_chat,
                        &self.default_embed,
                    );
                    if let Some(err) = &self.validation_error {
                        ui.colored_label(egui::Color32::from_rgb(220, 80, 80), err);
                    } else if !can_continue {
                        ui.colored_label(
                            egui::Color32::from_rgb(220, 80, 80),
                            t.model_setup_validation_need_both,
                        );
                    }

                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(can_continue, egui::Button::new(t.model_setup_download_continue))
                            .clicked()
                        {
                            self.try_confirm(&t);
                        }
                        if ui.button(t.model_setup_cancel).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_offer() -> ModelSetupOffer {
        ModelSetupOffer {
            hardware: HardwareInfo {
                gpu_name: "test".into(),
                vram_mib: 8192,
                tier: "mid".into(),
            },
            recommended_ids: vec!["chat-9b".into(), "embed-0.6b".into()],
            alternative_chat_ids: vec![],
            optional_ids: vec![],
            models: vec![
                ModelOffering {
                    id: "chat-9b".into(),
                    name: "Chat 9B".into(),
                    profiles: vec!["chat".into()],
                    bytes: 5_000_000_000,
                    optional: false,
                },
                ModelOffering {
                    id: "embed-0.6b".into(),
                    name: "Embed 0.6B".into(),
                    profiles: vec!["embed".into()],
                    bytes: 400_000_000,
                    optional: false,
                },
            ],
        }
    }

    #[test]
    fn empty_selection_is_invalid() {
        let offer = sample_offer();
        let selected = HashSet::new();
        assert!(!selection_valid(&offer, &selected, "", ""));
    }

    #[test]
    fn recommended_selection_is_valid() {
        let offer = sample_offer();
        let selected: HashSet<_> = offer.recommended_ids.iter().cloned().collect();
        assert!(selection_valid(&offer, &selected, "chat-9b", "embed-0.6b"));
    }

    #[test]
    fn remote_chat_with_local_embed_is_valid() {
        let offer = sample_offer();
        let selected: HashSet<_> = ["embed-0.6b".into()].into_iter().collect();
        assert!(selection_valid(
            &offer,
            &selected,
            "provider:openai:gpt-4o",
            "embed-0.6b"
        ));
    }

    #[test]
    fn resolve_defaults_picks_from_selected() {
        let offer = sample_offer();
        let selected: HashSet<_> = offer.recommended_ids.iter().cloned().collect();
        let (chat, embed) = resolve_defaults(&offer, &selected, "", "");
        assert_eq!(chat, "chat-9b");
        assert_eq!(embed, "embed-0.6b");
    }
}
