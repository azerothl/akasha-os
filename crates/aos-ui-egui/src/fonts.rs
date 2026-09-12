//! Interface font selection — bundled OFL faces plus the egui default stack.

use eframe::egui::{self, FontData, FontDefinitions, FontFamily};

/// English / French labels for each [`crate::prefs::UI_FONT_IDS`] entry.
pub const UI_FONT_LABELS: [(&str, &str); 4] = [
    ("Default", "Défaut"),
    ("Inter", "Inter"),
    ("Source Sans", "Source Sans"),
    ("Atkinson", "Atkinson"),
];

pub fn ui_font_label(id: &str, fr: bool) -> &'static str {
    let key = crate::prefs::normalize_ui_font(id);
    for (i, code) in crate::prefs::UI_FONT_IDS.iter().enumerate() {
        if *code == key {
            return if fr {
                UI_FONT_LABELS[i].1
            } else {
                UI_FONT_LABELS[i].0
            };
        }
    }
    UI_FONT_LABELS[0].0
}

fn default_fonts() -> FontDefinitions {
    FontDefinitions::default()
}

fn inject_bundled_font(fonts: &mut FontDefinitions, key: &str, data: FontData) {
    fonts
        .font_data
        .insert(key.to_owned(), std::sync::Arc::new(data));
    if let Some(prop) = fonts.families.get_mut(&FontFamily::Proportional) {
        prop.retain(|name| name != key);
        prop.insert(0, key.to_owned());
    }
}

/// Apply the user's interface font. Safe to call every frame (same pattern as theme).
pub fn apply_ui_font(ctx: &egui::Context, font_id: &str) {
    let choice = crate::prefs::normalize_ui_font(font_id);
    let mut fonts = default_fonts();
    match choice {
        "default" => {}
        "inter" => inject_bundled_font(
            &mut fonts,
            "inter",
            FontData::from_static(include_bytes!("../assets/fonts/Inter-Regular.ttf")),
        ),
        "source_sans" => inject_bundled_font(
            &mut fonts,
            "source_sans",
            FontData::from_static(include_bytes!("../assets/fonts/SourceSans3-Regular.otf")),
        ),
        "atkinson" => inject_bundled_font(
            &mut fonts,
            "atkinson",
            FontData::from_static(include_bytes!(
                "../assets/fonts/AtkinsonHyperlegible-Regular.ttf"
            )),
        ),
        _ => {}
    }
    ctx.set_fonts(fonts);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prefs;

    #[test]
    fn normalize_ui_font_maps_aliases() {
        assert_eq!(prefs::normalize_ui_font("inter"), "inter");
        assert_eq!(prefs::normalize_ui_font(" Source_Sans "), "source_sans");
        assert_eq!(prefs::normalize_ui_font("nope"), "default");
    }

    #[test]
    fn ui_font_label_respects_language() {
        assert_eq!(ui_font_label("default", false), "Default");
        assert_eq!(ui_font_label("default", true), "Défaut");
        assert_eq!(ui_font_label("inter", true), "Inter");
    }
}
