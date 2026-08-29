//! Morning skill suggestion card in the Chat thread (Preview 0.15).

use crate::cmd::Cmd;
use crate::i18n::UiStrings;
use aos_proto::ChatAttachment;
use eframe::egui;
use std::sync::mpsc::Sender;

pub fn label_for_lang(label_en: &str, label_fr: &str, lang: &str) -> String {
    if lang.starts_with("fr") {
        label_fr.to_string()
    } else {
        label_en.to_string()
    }
}

pub fn render_skill_offer_card(
    ui: &mut egui::Ui,
    t: &UiStrings,
    lang: &str,
    cmd_tx: &Sender<Cmd>,
    att: &mut ChatAttachment,
) -> bool {
    let ChatAttachment::SkillOffer {
        pattern_id,
        label_en,
        label_fr,
        state,
    } = att
    else {
        return false;
    };

    let label = label_for_lang(label_en, label_fr, lang);
    let title = label.trim().to_string();
    let mute = t.skill_offer_mute.replace("{label}", &label);

    egui::Frame::new()
        .fill(ui.visuals().widgets.inactive.bg_fill)
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(title).strong());
            ui.add_space(4.0);
            ui.label(egui::RichText::new(mute).weak());
            ui.add_space(8.0);
            match state.as_str() {
                "pending" => {
                    ui.horizontal(|ui| {
                        if ui.button(t.skill_offer_create).clicked() {
                            let _ = cmd_tx.send(Cmd::SkillPassCreate {
                                pattern_id: pattern_id.clone(),
                            });
                        }
                        if ui.button(t.skill_offer_later).clicked() {
                            *state = "dismissed".into();
                            let _ = cmd_tx.send(Cmd::SkillPassDismiss {
                                pattern_id: pattern_id.clone(),
                            });
                        }
                    });
                }
                "created" => {
                    ui.label(egui::RichText::new(t.skill_offer_created).weak());
                }
                "dismissed" => {
                    ui.label(egui::RichText::new(t.skill_offer_dismissed).weak());
                }
                other => {
                    ui.label(format!("{other}"));
                }
            }
        });
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n;

    #[test]
    fn card_copy_uses_label_not_internals() {
        let t_en = i18n::strings("en");
        let t_fr = i18n::strings("fr");
        let en = t_en.skill_offer_mute.replace("{label}", "weather");
        let fr = t_fr.skill_offer_mute.replace("{label}", "météo");
        assert!(en.contains("weather"));
        assert!(fr.contains("météo"));
        assert!(!en.contains("pat-"));
        assert!(!en.contains("body"));
        assert!(!en.contains("workflow"));
        assert!(!en.contains('{'));
    }

    #[test]
    fn label_for_lang_respects_pref() {
        assert_eq!(label_for_lang("weather", "météo", "en"), "weather");
        assert_eq!(label_for_lang("weather", "météo", "fr"), "météo");
    }

    #[test]
    fn button_copy_en_fr() {
        let en = i18n::strings("en");
        let fr = i18n::strings("fr");
        assert_eq!(en.skill_offer_create, "Create");
        assert_eq!(en.skill_offer_later, "Later");
        assert_eq!(fr.skill_offer_create, "Créer");
        assert_eq!(fr.skill_offer_later, "Plus tard");
    }
}
