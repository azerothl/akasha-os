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

    egui::Frame::new()
        .fill(ui.visuals().widgets.inactive.bg_fill)
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(title).strong());
            ui.add_space(4.0);
            let mute = match state.as_str() {
                "pending" => t.skill_offer_mute,
                "created" => t.skill_offer_created,
                "dismissed" => t.skill_offer_dismissed,
                other => other,
            };
            ui.label(egui::RichText::new(mute).weak());
            if state == "pending" {
                ui.add_space(8.0);
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
        });
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n;

    #[test]
    fn card_copy_mute_fixed_en_fr() {
        let t_en = i18n::strings("en");
        let t_fr = i18n::strings("fr");
        assert_eq!(
            t_en.skill_offer_mute,
            "You ask for this often. I can turn it into a skill."
        );
        assert_eq!(
            t_fr.skill_offer_mute,
            "Tu demandes souvent ça. Je peux en faire une skill."
        );
        assert!(!t_en.skill_offer_mute.contains('{'));
        assert!(!t_fr.skill_offer_mute.contains('{'));
    }

    #[test]
    fn label_for_lang_respects_pref() {
        assert_eq!(label_for_lang("weather", "météo", "en"), "weather");
        assert_eq!(label_for_lang("weather", "météo", "fr"), "météo");
    }

    #[test]
    fn resolved_state_copy_en_fr() {
        let en = i18n::strings("en");
        let fr = i18n::strings("fr");
        assert_eq!(en.skill_offer_created, "Created");
        assert_eq!(fr.skill_offer_created, "Créée");
        assert_eq!(en.skill_offer_dismissed, "Later");
        assert_eq!(fr.skill_offer_dismissed, "Plus tard");
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
