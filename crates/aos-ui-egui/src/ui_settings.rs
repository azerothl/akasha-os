//! Application settings panel.

use crate::cmd::Cmd;
use crate::onboarding::save_onboarding;
use crate::os_open::aos_home;
use crate::prefs::{save_preferences, UiDensity, UI_SCALE_PRESETS};
use crate::{i18n, Tab, UiApp};
use eframe::egui;

fn color_from_hex(value: &str) -> egui::Color32 {
    let raw = value.trim().trim_start_matches('#');
    if raw.len() != 6 {
        return egui::Color32::WHITE;
    }
    let Ok(rgb) = u32::from_str_radix(raw, 16) else {
        return egui::Color32::WHITE;
    };
    egui::Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

fn edit_theme_color(ui: &mut egui::Ui, label: &str, value: &mut String) -> bool {
    let mut color = color_from_hex(value);
    let changed = ui.color_edit_button_srgba(&mut color).changed();
    ui.monospace(value.as_str());
    if changed {
        *value = format!("#{:02X}{:02X}{:02X}", color.r(), color.g(), color.b());
    }
    ui.label(label);
    changed
}

impl UiApp {
    pub(crate) fn ui_settings(&mut self, ui: &mut egui::Ui) {
        let t = i18n::strings(&self.prefs.language);
        ui.heading(t.settings_title);
        crate::ui_primitives::search_field(ui, &mut self.settings_ui.search, t.settings_search_hint);
        ui.separator();

        let label_w = 160.0_f32;
        let query = self.settings_ui.search.trim().to_lowercase();
        let section_visible = |terms: &[&str]| {
            query.is_empty() || terms.iter().any(|term| term.contains(query.as_str()))
        };
        if !query.is_empty()
            && ![
                ["utilisateur", "user", "langue", "language", "thème", "theme", "densité", "density", "échelle", "scale"].as_slice(),
                ["modèle", "model", "inference", "routage", "routing", "image", "audio"].as_slice(),
                ["confidentialité", "privacy", "confiance", "trust", "réseau", "network", "mémoire", "remember"].as_slice(),
                ["agent", "expert", "étapes", "steps", "timeout"].as_slice(),
                ["web", "recherche", "search", "navigation", "browse"].as_slice(),
                ["secret", "clé", "token", "api"].as_slice(),
                ["catalogue", "catalog", "module", "community", "communauté"].as_slice(),
                ["planification", "schedule", "tâche", "task"].as_slice(),
            ]
            .iter()
            .any(|terms| terms.iter().any(|term| term.contains(query.as_str())))
        {
            ui.weak(t.settings_search_empty);
            return;
        }

        if section_visible(&["utilisateur", "user", "langue", "language", "thème", "theme", "densité", "density", "échelle", "scale"]) {
        ui.heading(t.settings_me);
        egui::Grid::new("settings_me")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .min_col_width(label_w)
            .show(ui, |ui| {
                ui.label(t.language);
                ui.horizontal(|ui| {
                    for (code, label) in [("en", "English"), ("fr", "Français")] {
                        if ui
                            .selectable_label(self.prefs.language == code, label)
                            .clicked()
                        {
                            self.prefs.language = code.into();
                            self.onboarding.language = code.into();
                            save_preferences(&self.prefs);
                            save_onboarding(&self.onboarding);
                            self.status = t.settings_saved.into();
                        }
                    }
                });
                ui.end_row();

                ui.label(t.theme);
                let theme_label = match self.prefs.theme.as_str() {
                    "light" => t.theme_light,
                    "soft" => t.theme_soft,
                    "high_contrast" => t.theme_high_contrast,
                    "custom" => t.settings_theme_custom,
                    _ => t.theme_dark,
                };
                egui::ComboBox::from_id_salt("prefs_theme")
                    .selected_text(theme_label)
                    .show_ui(ui, |ui| {
                        for (code, label) in [
                            ("dark", t.theme_dark),
                            ("light", t.theme_light),
                            ("soft", t.theme_soft),
                            ("high_contrast", t.theme_high_contrast),
                            ("custom", t.settings_theme_custom),
                        ] {
                            if ui
                                .selectable_label(self.prefs.theme == code, label)
                                .clicked()
                            {
                                self.prefs.theme = code.into();
                                save_preferences(&self.prefs);
                                self.status = t.settings_saved.into();
                            }
                        }
                    });
                ui.end_row();

                if self.prefs.theme == "custom" {
                    ui.end_row();
                    ui.label(t.settings_custom_colors);
                    egui::Grid::new("custom_theme_colors")
                        .num_columns(3)
                        .spacing([12.0, 8.0])
                        .show(ui, |ui| {
                            let mut changed = false;
                            changed |= edit_theme_color(ui, t.settings_color_background, &mut self.prefs.custom_theme.background);
                            ui.end_row();
                            changed |= edit_theme_color(ui, t.settings_color_panel, &mut self.prefs.custom_theme.panel);
                            ui.end_row();
                            changed |= edit_theme_color(ui, t.settings_color_text, &mut self.prefs.custom_theme.text);
                            ui.end_row();
                            changed |= edit_theme_color(ui, t.settings_color_accent, &mut self.prefs.custom_theme.accent);
                            ui.end_row();
                            changed |= edit_theme_color(ui, t.settings_color_danger, &mut self.prefs.custom_theme.danger);
                            ui.end_row();
                            if changed {
                                save_preferences(&self.prefs);
                            }
                        });
                    ui.weak(t.settings_colors_applied);
                    ui.end_row();
                }

                ui.label(t.settings_ui_scale);
                let scale_label = format!("{}%", self.prefs.ui_scale_percent);
                egui::ComboBox::from_id_salt("prefs_ui_scale")
                    .selected_text(scale_label)
                    .show_ui(ui, |ui| {
                        for percent in UI_SCALE_PRESETS {
                            let label = format!("{percent}%");
                            if ui
                                .selectable_label(self.prefs.ui_scale_percent == percent, label)
                                .on_hover_text(t.settings_ui_scale_hint)
                                .clicked()
                            {
                                self.prefs.ui_scale_percent = percent;
                                save_preferences(&self.prefs);
                                self.status = t.settings_saved.into();
                            }
                        }
                    });
                ui.end_row();

                ui.label(t.settings_density);
                ui.horizontal(|ui| {
                    for (density, label) in [
                        (UiDensity::Comfortable, t.settings_density_comfortable),
                        (UiDensity::Compact, t.settings_density_compact),
                    ] {
                        if ui
                            .selectable_label(self.prefs.ui_density == density, label)
                            .clicked()
                        {
                            self.prefs.ui_density = density;
                            save_preferences(&self.prefs);
                            self.status = t.settings_saved.into();
                        }
                    }
                });
                ui.end_row();

                ui.label(t.settings_auto_download_updates);
                let mut auto_upd = self.prefs.auto_download_updates;
                if ui
                    .checkbox(&mut auto_upd, t.settings_auto_download_updates)
                    .on_hover_text(t.settings_auto_download_updates_hint)
                    .changed()
                {
                    self.prefs.auto_download_updates = auto_upd;
                    save_preferences(&self.prefs);
                    self.status = t.settings_saved.into();
                }
                ui.end_row();
            });
        }

        ui.add_space(12.0);
        if section_visible(&["modèle", "model", "inference", "routage", "routing", "image", "audio"]) {
        ui.heading(t.settings_models);
        egui::Grid::new("settings_models")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .min_col_width(label_w)
            .show(ui, |ui| {
                ui.label(t.inference_mode);
                ui.horizontal(|ui| {
                    for (code, label) in [
                        ("auto", "Auto"),
                        ("gpu", t.inference_gpu),
                        ("cpu", t.inference_cpu),
                    ] {
                        if ui
                            .selectable_label(self.prefs.inference_mode == code, label)
                            .clicked()
                        {
                            self.prefs.inference_mode = code.into();
                            save_preferences(&self.prefs);
                            let _ = self.cmd_tx.send(Cmd::MigrateModeld {
                                target: code.to_string(),
                            });
                            self.status = format!("{} — migrate ({code})", t.settings_saved);
                        }
                    }
                });
                ui.end_row();

                ui.label(t.routing);
                ui.horizontal(|ui| {
                    for code in ["local_only", "balanced", "remote_only"] {
                        let label = i18n::routing_label(&t, code);
                        let tech = i18n::routing_technical(&t, code);
                        if ui
                            .selectable_label(self.prefs.routing == code, label)
                            .on_hover_text(tech)
                            .clicked()
                        {
                            self.prefs.routing = code.into();
                            self.onboarding.routing = code.into();
                            save_preferences(&self.prefs);
                            save_onboarding(&self.onboarding);
                            let _ = self.cmd_tx.send(Cmd::SetRouting {
                                mode: code.to_string(),
                            });
                        }
                    }
                });
                ui.end_row();

                ui.label(t.settings_default_model);
                egui::ComboBox::from_id_salt("prefs_agent_model")
                    .selected_text(
                        self.prefs
                            .default_agent_model
                            .clone()
                            .unwrap_or_else(|| "default".into()),
                    )
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(self.prefs.default_agent_model.is_none(), "default")
                            .clicked()
                        {
                            self.prefs.default_agent_model = None;
                            self.agent_ui.model_id.clear();
                            save_preferences(&self.prefs);
                        }
                        for m in self.models_ui.model_infos.clone() {
                            let selected =
                                self.prefs.default_agent_model.as_deref() == Some(m.id.as_str());
                            if ui.selectable_label(selected, &m.id).clicked() {
                                self.prefs.default_agent_model = Some(m.id.clone());
                                self.agent_ui.model_id = m.id;
                                save_preferences(&self.prefs);
                            }
                        }
                    });
                ui.end_row();

                ui.label(t.settings_image_pack);
                egui::ComboBox::from_id_salt("prefs_image_pack")
                    .selected_text(
                        self.prefs
                            .default_image_model
                            .clone()
                            .unwrap_or_else(|| "default".into()),
                    )
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(self.prefs.default_image_model.is_none(), "default")
                            .clicked()
                        {
                            self.prefs.default_image_model = None;
                            save_preferences(&self.prefs);
                        }
                        for m in self.models_ui.model_infos.clone() {
                            if !(m.id.contains("sd-")
                                || m.id.contains("flux")
                                || m.id.contains("ideogram")
                                || m.name.to_ascii_lowercase().contains("image"))
                            {
                                continue;
                            }
                            let selected =
                                self.prefs.default_image_model.as_deref() == Some(m.id.as_str());
                            if ui.selectable_label(selected, &m.id).clicked() {
                                self.prefs.default_image_model = Some(m.id.clone());
                                save_preferences(&self.prefs);
                            }
                        }
                    });
                ui.end_row();

                ui.label(t.settings_piper_voice);
                egui::ComboBox::from_id_salt("prefs_piper")
                    .selected_text(
                        self.prefs
                            .default_audio_model
                            .clone()
                            .unwrap_or_else(|| "default".into()),
                    )
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(self.prefs.default_audio_model.is_none(), "default")
                            .clicked()
                        {
                            self.prefs.default_audio_model = None;
                            save_preferences(&self.prefs);
                        }
                        for m in self.models_ui.model_infos.clone() {
                            if !m.id.contains("piper") {
                                continue;
                            }
                            let selected =
                                self.prefs.default_audio_model.as_deref() == Some(m.id.as_str());
                            if ui.selectable_label(selected, &m.id).clicked() {
                                self.prefs.default_audio_model = Some(m.id.clone());
                                save_preferences(&self.prefs);
                            }
                        }
                    });
                ui.end_row();

                ui.horizontal(|ui| {
                    if ui.button(t.tab_models).clicked() {
                        self.tab = Tab::Models;
                    }
                    if ui
                        .button(t.tab_providers)
                        .on_hover_text(t.tab_hint_providers)
                        .clicked()
                    {
                        self.tab = Tab::Providers;
                    }
                });
                ui.end_row();
            });
        }

        if section_visible(&["image", "expert", "steps", "taille", "size"]) {
        egui::CollapsingHeader::new(t.settings_expert_image_defaults)
            .default_open(false)
            .show(ui, |ui| {
                egui::Grid::new("settings_image_defaults")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .min_col_width(label_w)
                    .show(ui, |ui| {
                        ui.label("W / H / steps");
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::DragValue::new(&mut self.prefs.image_width).range(64..=2048),
                            );
                            ui.add(
                                egui::DragValue::new(&mut self.prefs.image_height).range(64..=2048),
                            );
                            if ui
                                .add(
                                    egui::DragValue::new(&mut self.prefs.image_steps)
                                        .range(1..=150),
                                )
                                .changed()
                            {
                                save_preferences(&self.prefs);
                            }
                            if ui.button(t.settings_saved).clicked() {
                                save_preferences(&self.prefs);
                            }
                        });
                        ui.end_row();
                    });
            });
        }

        ui.add_space(12.0);
        if section_visible(&["confidentialité", "privacy", "confiance", "trust", "réseau", "network", "mémoire", "remember"]) {
        ui.heading(t.settings_trust);
        egui::Grid::new("settings_trust")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .min_col_width(label_w)
            .show(ui, |ui| {
                ui.label(t.trust_default);
                ui.horizontal(|ui| {
                    for (code, label) in [("low", t.trust_low), ("medium", t.trust_medium)] {
                        if ui
                            .selectable_label(self.prefs.trust_default == code, label)
                            .clicked()
                        {
                            self.prefs.trust_default = code.into();
                            self.onboarding.trust_default = code.into();
                            save_preferences(&self.prefs);
                            save_onboarding(&self.onboarding);
                        }
                    }
                });
                ui.end_row();

                ui.label(t.network_heading);
                let mut online = self.prefs.network_online;
                if ui.checkbox(&mut online, t.allow_network).changed() {
                    self.prefs.network_online = online;
                    self.network_online = online;
                    save_preferences(&self.prefs);
                    let _ = self.cmd_tx.send(Cmd::NetSetMode { online });
                }
                ui.end_row();

                ui.label(t.settings_auto_remember);
                let mut auto = self.prefs.auto_remember_chat;
                if ui
                    .checkbox(&mut auto, t.settings_auto_remember)
                    .on_hover_text(t.settings_auto_remember_hint)
                    .changed()
                {
                    self.prefs.auto_remember_chat = auto;
                    save_preferences(&self.prefs);
                    self.status = t.settings_saved.into();
                }
                ui.end_row();
            });
        }

        if section_visible(&["agent", "expert", "étapes", "steps", "timeout"]) {
        egui::CollapsingHeader::new(t.settings_expert_agent)
            .default_open(false)
            .show(ui, |ui| {
                egui::Grid::new("settings_agents")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .min_col_width(label_w)
                    .show(ui, |ui| {
                        ui.label(t.settings_max_steps);
                        if ui
                            .add(
                                egui::DragValue::new(&mut self.prefs.default_max_steps)
                                    .range(1..=128),
                            )
                            .changed()
                        {
                            self.agent_ui.max_steps = self.prefs.default_max_steps;
                            save_preferences(&self.prefs);
                        }
                        ui.end_row();

                        ui.label(t.settings_timeout);
                        if ui
                            .add(
                                egui::DragValue::new(&mut self.prefs.default_timeout_secs)
                                    .range(60..=86_400),
                            )
                            .changed()
                        {
                            self.agent_ui.timeout_secs = self.prefs.default_timeout_secs;
                            save_preferences(&self.prefs);
                        }
                        ui.end_row();
                    });
            });
        }

        if section_visible(&["web", "recherche", "search", "navigation", "browse"]) {
        egui::CollapsingHeader::new(t.settings_expert_web)
            .default_open(false)
            .show(ui, |ui| {
                egui::Grid::new("settings_web")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .min_col_width(label_w)
                    .show(ui, |ui| {
                        ui.label(t.settings_search_engine);
                        egui::ComboBox::from_id_salt("prefs_search_engine")
                            .selected_text(&self.prefs.web_search_engine)
                            .show_ui(ui, |ui| {
                                for eng in ["auto", "brave", "searxng", "duckduckgo", "bing"] {
                                    if ui
                                        .selectable_label(self.prefs.web_search_engine == eng, eng)
                                        .clicked()
                                    {
                                        self.prefs.web_search_engine = eng.into();
                                        save_preferences(&self.prefs);
                                    }
                                }
                            });
                        ui.end_row();

                        ui.label(t.settings_searxng_url);
                        if ui
                            .add(
                                egui::TextEdit::singleline(&mut self.prefs.searxng_url)
                                    .desired_width(220.0)
                                    .hint_text("https://searx.example"),
                            )
                            .changed()
                        {
                            save_preferences(&self.prefs);
                        }
                        ui.end_row();
                        ui.label("");
                        ui.weak(t.settings_searxng_hint);
                        ui.end_row();

                        ui.label(t.settings_browse_chars);
                        if ui
                            .add(
                                egui::DragValue::new(&mut self.prefs.web_browse_max_chars)
                                    .range(1000..=100_000),
                            )
                            .changed()
                        {
                            save_preferences(&self.prefs);
                        }
                        ui.end_row();

                        ui.label(t.settings_fetch_max);
                        if ui
                            .add(
                                egui::DragValue::new(&mut self.prefs.web_fetch_max_bytes)
                                    .range(1024..=200_000_000),
                            )
                            .changed()
                        {
                            save_preferences(&self.prefs);
                        }
                        ui.end_row();
                    });
            });
        }

        if section_visible(&["secret", "clé", "token", "api"]) {
        egui::CollapsingHeader::new(t.settings_secrets)
            .default_open(false)
            .show(ui, |ui| {
                ui.weak(t.settings_secrets_blurb);
                egui::Grid::new("settings_secrets")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .min_col_width(label_w)
                    .show(ui, |ui| {
                        ui.label("Brave Search");
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.settings_ui.secret_brave)
                                    .password(true)
                                    .desired_width(220.0)
                                    .hint_text("BSA…"),
                            );
                            if ui.button(t.settings_secret_save).clicked() {
                                let _ = self.cmd_tx.send(Cmd::SecretSet {
                                    name: "brave_search_api_key".into(),
                                    value: self.settings_ui.secret_brave.clone(),
                                });
                                self.settings_ui.secret_brave.clear();
                            }
                        });
                        ui.end_row();

                        ui.label("GitHub token");
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.settings_ui.secret_github)
                                    .password(true)
                                    .desired_width(220.0)
                                    .hint_text("ghp_…"),
                            );
                            if ui.button(t.settings_secret_save).clicked() {
                                let _ = self.cmd_tx.send(Cmd::SecretSet {
                                    name: "github_token".into(),
                                    value: self.settings_ui.secret_github.clone(),
                                });
                                self.settings_ui.secret_github.clear();
                            }
                        });
                        ui.end_row();

                        ui.label(t.settings_secret_openai);
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.settings_ui.secret_openai)
                                    .password(true)
                                    .desired_width(220.0)
                                    .hint_text("sk-…"),
                            );
                            if ui.button(t.settings_secret_save).clicked() {
                                let _ = self.cmd_tx.send(Cmd::SecretSet {
                                    name: "openai_api_key".into(),
                                    value: self.settings_ui.secret_openai.clone(),
                                });
                                self.settings_ui.secret_openai.clear();
                            }
                        });
                        ui.end_row();
                    });
                ui.horizontal(|ui| {
                    if ui.button(t.settings_secret_list).clicked() {
                        let _ = self.cmd_tx.send(Cmd::SecretList);
                    }
                    if self.settings_ui.secret_vault_encrypted {
                        ui.weak(t.settings_secret_encrypted);
                    }
                    if !self.settings_ui.secret_names.is_empty() {
                        ui.weak(format!(
                            "{}: {}",
                            t.settings_secret_configured,
                            self.settings_ui.secret_names.join(", ")
                        ));
                    }
                });
                ui.weak(t.settings_brave_hint);
            });
        }

        if section_visible(&["catalogue", "catalog", "module", "community", "communauté"]) {
        egui::CollapsingHeader::new(t.settings_catalogue)
            .default_open(false)
            .show(ui, |ui| {
                ui.weak(t.settings_catalogue_blurb);
                if ui.button(t.settings_secret_list).clicked() {
                    let _ = self.cmd_tx.send(Cmd::CatalogueRefresh);
                    let _ = self.cmd_tx.send(Cmd::ModuleList);
                }
                ui.add_space(6.0);
                ui.weak(t.settings_catalogue_community_blurb);
                let mut community_on = self.prefs.community_catalogue_enabled;
                if ui
                    .checkbox(&mut community_on, t.settings_catalogue_community_enable)
                    .changed()
                {
                    self.prefs.community_catalogue_enabled = community_on;
                    save_preferences(&self.prefs);
                    let _ = self.cmd_tx.send(Cmd::CatalogueSetSource {
                        enabled: community_on,
                    });
                }
                if community_on && ui.button(t.settings_catalogue_community_fetch).clicked() {
                    let _ = self.cmd_tx.send(Cmd::CatalogueFetchExtra);
                }
                if let Some(cat) = &self.settings_ui.catalogue {
                    if cat.extra_enabled && cat.extra_cached && cat.extra_signature_ok {
                        ui.weak(t.settings_catalogue_community_cached);
                    }
                    if cat.extra_enabled && !cat.extra_error.is_empty() {
                        ui.weak(format!(
                            "{} ({})",
                            t.settings_catalogue_community_unsigned, cat.extra_error
                        ));
                    } else if cat.extra_enabled && !cat.extra_signature_ok {
                        ui.weak(t.settings_catalogue_community_unsigned);
                    }
                }
                match self.settings_ui.catalogue.clone() {
                    Some(cat) if cat.signature_ok || cat.extra_signature_ok => {
                        for e in cat.entries {
                            let source_label = if e.source == "community" {
                                t.settings_catalogue_source_community
                            } else {
                                t.settings_catalogue_source_bundled
                            };
                            let installed_mod = self
                                .settings_ui
                                .installed_modules
                                .iter()
                                .find(|m| m.name == e.name)
                                .cloned();
                            let skill_installed = self
                                .settings_ui
                                .installed_skills
                                .iter()
                                .any(|n| n == &e.name);
                            ui.horizontal(|ui| {
                                let mut label = format!(
                                    "{} {} ({}) [{}]",
                                    e.name, e.version, e.kind, source_label
                                );
                                if !e.license.is_empty() {
                                    label.push_str(&format!(" {}", e.license));
                                }
                                if installed_mod.is_some() || skill_installed {
                                    label.push_str(&format!(
                                        " [{}]",
                                        t.settings_catalogue_installed
                                    ));
                                    if installed_mod
                                        .as_ref()
                                        .map(|m| m.quarantined)
                                        .unwrap_or(false)
                                    {
                                        label.push_str(" [quarantine]");
                                    }
                                }
                                ui.label(label);
                                match e.kind.as_str() {
                                    "module" => {
                                        if aos_proto::decl_ui::is_bundled_module(&e.name) {
                                            ui.weak(t.settings_bundled_locked);
                                        } else if installed_mod.is_some() {
                                            if ui.button(t.settings_catalogue_uninstall).clicked() {
                                                let _ = self.cmd_tx.send(Cmd::ModuleUninstall {
                                                    name: e.name.clone(),
                                                });
                                            }
                                        } else if e.source == "community" {
                                            if ui.button(t.settings_catalogue_install).clicked() {
                                                let _ = self.cmd_tx.send(Cmd::CatalogueInstall {
                                                    name: e.name.clone(),
                                                });
                                            }
                                        } else if ui.button(t.settings_catalogue_install).clicked()
                                        {
                                            let src = aos_home().join(&e.path);
                                            let _ = self.cmd_tx.send(Cmd::ModuleInstall {
                                                source_dir: src.to_string_lossy().into_owned(),
                                                approved_caps: None,
                                            });
                                        }
                                    }
                                    "skill" => {
                                        if skill_installed {
                                            if ui.button(t.settings_catalogue_uninstall).clicked() {
                                                let _ = self.cmd_tx.send(Cmd::SkillUninstall {
                                                    name: e.name.clone(),
                                                });
                                            }
                                        } else if ui.button(t.settings_catalogue_install).clicked()
                                        {
                                            let _ = self.cmd_tx.send(Cmd::CatalogueInstall {
                                                name: e.name.clone(),
                                            });
                                        }
                                    }
                                    _ => {}
                                }
                            });
                            if !e.attested_caps.is_empty() {
                                ui.weak(format!(
                                    "{}: {}",
                                    t.settings_catalogue_caps,
                                    e.attested_caps.join(", ")
                                ));
                            }
                        }
                    }
                    Some(_) => {
                        ui.weak(t.settings_catalogue_unsigned);
                    }
                    None => {
                        ui.weak(t.settings_catalogue_unsigned);
                    }
                }

                ui.add_space(8.0);
                ui.weak(t.settings_installed_modules);
                for m in self.settings_ui.installed_modules.clone() {
                    ui.horizontal(|ui| {
                        ui.label(format!("{} v{}", m.name, m.version));
                        if aos_proto::decl_ui::is_bundled_module(&m.name) {
                            ui.weak(t.settings_bundled_locked);
                        } else if ui.button(t.settings_catalogue_uninstall).clicked() {
                            let _ = self.cmd_tx.send(Cmd::ModuleUninstall {
                                name: m.name.clone(),
                            });
                        }
                    });
                }
            });
        }

        if section_visible(&["planification", "schedule", "tâche", "task"]) {
        egui::CollapsingHeader::new(t.schedule_heading)
            .default_open(false)
            .show(ui, |ui| {
                egui::Grid::new("settings_schedules")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .min_col_width(label_w)
                    .show(ui, |ui| {
                        ui.label(t.schedule_goal);
                        ui.add(
                            egui::TextEdit::singleline(&mut self.settings_ui.schedule_goal)
                                .desired_width(280.0)
                                .hint_text("agent goal"),
                        );
                        ui.end_row();

                        ui.label(t.schedule_interval);
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::DragValue::new(&mut self.settings_ui.schedule_interval_secs)
                                    .range(30..=86_400)
                                    .suffix(" s"),
                            );
                            if ui
                                .button(t.schedule_create)
                                .on_hover_text(t.tip_schedule_create)
                                .clicked()
                            {
                                self.send_settings_schedule_create();
                            }
                            if ui.button(t.caps_refresh).clicked() {
                                let _ = self.cmd_tx.send(Cmd::ScheduleList);
                            }
                        });
                        ui.end_row();
                    });
                if self.schedule_ui.entries.is_empty() {
                    ui.weak("Aucun schedule");
                } else {
                    for s in self.schedule_ui.entries.clone() {
                        ui.horizontal(|ui| {
                            let flag = if s.enabled { "ON" } else { "OFF" };
                            ui.monospace(&s.id);
                            ui.label(format!(
                                "[{flag}] every {}s · fires={} · {}",
                                s.interval_secs, s.fire_count, s.goal
                            ));
                            if s.enabled
                                && ui
                                    .small_button(t.schedule_cancel)
                                    .on_hover_text(t.tip_schedule_cancel)
                                    .clicked()
                            {
                                let _ = self.cmd_tx.send(Cmd::ScheduleCancel { id: s.id });
                            }
                        });
                    }
                }
            });
        }
    }
}
