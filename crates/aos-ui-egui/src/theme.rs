//! Cloud-chamber design tokens mapped to egui Visuals (see docs/UI.md).

use eframe::egui;

pub const VOID: egui::Color32 = egui::Color32::from_rgb(7, 11, 20);
pub const ICE_TRACK: egui::Color32 = egui::Color32::from_rgb(94, 231, 255);
pub const SIGNAL: egui::Color32 = egui::Color32::from_rgb(46, 240, 200);
pub const HYDROGEN: egui::Color32 = egui::Color32::from_rgb(255, 90, 72);
pub const PAPER: egui::Color32 = egui::Color32::from_rgb(232, 238, 246);

/// Shared UI tokens.  Keeping these here prevents individual panels from
/// inventing their own hit targets and makes accessibility regressions easy to
/// test.
///
/// Token sources: Tailwind-scale spacing + Lucide 24px glyph grid
/// (see opensourceui.in mapping — MIT, bidyut10/opensourceui).
/// Only tokens live here; widgets live in `icons.rs` / panels.
pub const CONTROL_MIN_H_COMFORTABLE: f32 = 36.0;
#[allow(dead_code)]
pub const CONTROL_MIN_H_COMPACT: f32 = 32.0;
/// Primary CTA height (Envoyer, Save, Get Preview). WCAG target §2.5.8
/// recommends 24px minimum, 44px preferred for pointer inputs.
#[allow(dead_code)]
pub const PRIMARY_MIN_H: f32 = 44.0;
/// Accessible hit target for icon-only controls. Glyph stays 18px centered.
pub const ICON_HIT: f32 = 28.0;
pub const ICON_GLYPH: f32 = 18.0;
/// Toolbar hit target (canvas tools, session bar toggles).
pub const TOOLBAR_HIT: f32 = 32.0;
#[allow(dead_code)]
pub const COMPOSER_MIN_H: f32 = 44.0;
#[allow(dead_code)]
pub const CARD_RADIUS: u8 = 8;
#[allow(dead_code)]
pub const RADIUS_SM: u8 = 6;
#[allow(dead_code)]
pub const RADIUS_MD: u8 = 8;
#[allow(dead_code)]
pub const RADIUS_LG: u8 = 12;
#[allow(dead_code)]
pub const STROKE_SUBTLE: f32 = 1.0;
pub const SPACE_UNIT: f32 = 8.0;

/// Semantic fills ported from opensourceui tactile/soft-ui buttons.
/// Valeurs canoniques (dark). Ne pas les référencer directement dans les
/// widgets : passer par `button_colors(ui)` pour que `light/soft/
/// high_contrast/custom` puissent les surcharger.
#[allow(dead_code)]
pub const SUCCESS: egui::Color32 = egui::Color32::from_rgb(52, 211, 153);
#[allow(dead_code)]
pub const WARNING: egui::Color32 = egui::Color32::from_rgb(251, 191, 36);
/// Translucent scrim for spotlight/toast overlays (not const: `from_rgba_*`
/// is not const in egui 0.31, use this helper).
#[allow(dead_code)]
pub fn scrim() -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(7, 11, 20, 180)
}

/// Couleurs sémantiques résolues pour le thème courant. Stockées en mémoire
/// egui à chaque `apply_theme` pour que les boutons/toasts restent
/// surchargables (custom inclus) sans couleur en dur dans les widgets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemeColors {
    pub accent: egui::Color32,
    pub danger: egui::Color32,
    pub success: egui::Color32,
    pub warning: egui::Color32,
}

fn theme_colors_id() -> egui::Id {
    egui::Id::new("aos_theme_colors")
}

pub fn theme_colors(
    theme: &str,
    custom: &crate::prefs::CustomThemePreferences,
) -> ThemeColors {
    match theme {
        "light" => ThemeColors {
            accent: mix(VOID, SIGNAL, 0.85),
            danger: mix(HYDROGEN, VOID, 0.25),
            success: mix(SUCCESS, VOID, 0.35),
            warning: mix(WARNING, VOID, 0.35),
        },
        "soft" => ThemeColors {
            accent: mix(SIGNAL, VOID, 0.15),
            danger: mix(HYDROGEN, VOID, 0.25),
            success: mix(SUCCESS, VOID, 0.35),
            warning: mix(WARNING, VOID, 0.35),
        },
        "high_contrast" => ThemeColors {
            accent: SIGNAL,
            danger: HYDROGEN,
            success: SUCCESS,
            warning: WARNING,
        },
        "custom" => ThemeColors {
            accent: parse_hex(&custom.accent, SIGNAL),
            danger: parse_hex(&custom.danger, HYDROGEN),
            success: parse_hex(&custom.success, SUCCESS),
            warning: parse_hex(&custom.warning, WARNING),
        },
        _ => ThemeColors {
            accent: SIGNAL,
            danger: HYDROGEN,
            success: SUCCESS,
            warning: WARNING,
        },
    }
}

/// Couleurs boutons/toasts pour le frame courant. Fallback dark si
/// `apply_theme` n'a pas encore tourné (tests).
pub fn button_colors(ui: &egui::Ui) -> ThemeColors {
    ui.ctx()
        .memory_mut(|m| m.data.get_persisted::<ThemeColors>(theme_colors_id()))
        .unwrap_or(ThemeColors {
            accent: SIGNAL,
            danger: HYDROGEN,
            success: SUCCESS,
            warning: WARNING,
        })
}

const FOCUS_STROKE_WIDTH: f32 = 2.0;

fn mix(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let t = t.clamp(0.0, 1.0);
    let inv = 1.0 - t;
    egui::Color32::from_rgb(
        (f32::from(a.r()) * inv + f32::from(b.r()) * t) as u8,
        (f32::from(a.g()) * inv + f32::from(b.g()) * t) as u8,
        (f32::from(a.b()) * inv + f32::from(b.b()) * t) as u8,
    )
}

#[cfg(test)]
fn relative_luminance(c: egui::Color32) -> f32 {
    fn channel(v: u8) -> f32 {
        let s = f32::from(v) / 255.0;
        if s <= 0.03928 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * channel(c.r()) + 0.7152 * channel(c.g()) + 0.0722 * channel(c.b())
}

/// WCAG 2.x contrast ratio between two sRGB colors.
#[cfg(test)]
pub fn contrast_ratio(fg: egui::Color32, bg: egui::Color32) -> f32 {
    let l1 = relative_luminance(fg);
    let l2 = relative_luminance(bg);
    let (lighter, darker) = if l1 >= l2 { (l1, l2) } else { (l2, l1) };
    (lighter + 0.05) / (darker + 0.05)
}

fn apply_focus_ring(v: &mut egui::Visuals, accent: egui::Color32) {
    let focus = egui::Stroke::new(FOCUS_STROKE_WIDTH, accent);
    v.widgets.active.bg_stroke = focus;
    v.widgets.active.fg_stroke.width = FOCUS_STROKE_WIDTH;
    v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, accent);
}

fn base_widgets(v: &mut egui::Visuals, fg: egui::Color32, accent: egui::Color32) {
    v.override_text_color = Some(fg);
    v.selection.bg_fill = accent;
    v.selection.stroke = egui::Stroke::new(FOCUS_STROKE_WIDTH, VOID);
    v.hyperlink_color = accent;
    v.warn_fg_color = HYDROGEN;
    v.error_fg_color = HYDROGEN;
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.fg_stroke.color = fg;
    }
    apply_focus_ring(v, accent);
}

pub fn apply_theme(
    ctx: &egui::Context,
    theme: &str,
    custom: &crate::prefs::CustomThemePreferences,
) {
    let visuals = match theme {
        "light" => chamber_light(),
        "soft" => chamber_soft(),
        "high_contrast" => chamber_high_contrast(),
        "custom" => chamber_custom(custom),
        _ => chamber_dark(),
    };
    ctx.set_visuals(visuals);
    let colors = theme_colors(theme, custom);
    ctx.memory_mut(|m| m.data.insert_persisted(theme_colors_id(), colors));
}

/// Scale the whole Preview chrome (rail, status bar, panels) via egui zoom factor.
/// Uses `zoom_factor` so OS/monitor DPI changes still apply at runtime.
pub fn apply_ui_scale(ctx: &egui::Context, scale_percent: u32) {
    let factor = crate::prefs::ui_scale_factor(scale_percent);
    if (ctx.zoom_factor() - factor).abs() > f32::EPSILON {
        ctx.set_zoom_factor(factor);
    }
}

fn parse_hex(value: &str, fallback: egui::Color32) -> egui::Color32 {
    let raw = value.trim().trim_start_matches('#');
    if raw.len() != 6 { return fallback; }
    let Ok(rgb) = u32::from_str_radix(raw, 16) else { return fallback; };
    egui::Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

fn chamber_custom(custom: &crate::prefs::CustomThemePreferences) -> egui::Visuals {
    let background = parse_hex(&custom.background, VOID);
    let panel = parse_hex(&custom.panel, mix(VOID, PAPER, 0.04));
    let text = parse_hex(&custom.text, PAPER);
    let accent = parse_hex(&custom.accent, SIGNAL);
    let danger = parse_hex(&custom.danger, HYDROGEN);
    let mut v = egui::Visuals::dark();
    v.dark_mode = true;
    v.extreme_bg_color = background;
    v.panel_fill = background;
    v.window_fill = panel;
    v.faint_bg_color = panel;
    v.widgets.noninteractive.bg_fill = panel;
    v.widgets.inactive.bg_fill = mix(panel, accent, 0.10);
    v.widgets.hovered.bg_fill = mix(panel, accent, 0.18);
    v.widgets.active.bg_fill = mix(panel, accent, 0.28);
    base_widgets(&mut v, text, accent);
    v.warn_fg_color = danger;
    v.error_fg_color = danger;
    v
}

/// Apply the product-wide density without allowing compact mode to create
/// touch targets smaller than 32 px.
pub fn apply_ui_density(ctx: &egui::Context, density: crate::prefs::UiDensity) {
    let mut style = (*ctx.style()).clone();
    let h = density.control_height();
    style.spacing.interact_size.y = h;
    style.spacing.button_padding = egui::vec2(16.0, ((h - 20.0) / 2.0).max(6.0));
    style.spacing.item_spacing = egui::vec2(SPACE_UNIT, SPACE_UNIT);
    ctx.set_style(style);
}

fn chamber_dark() -> egui::Visuals {
    let mut v = egui::Visuals::dark();
    v.dark_mode = true;
    v.extreme_bg_color = VOID;
    v.panel_fill = VOID;
    v.window_fill = mix(VOID, PAPER, 0.04);
    v.faint_bg_color = mix(VOID, ICE_TRACK, 0.08);
    v.widgets.noninteractive.bg_fill = mix(VOID, ICE_TRACK, 0.05);
    v.widgets.inactive.bg_fill = mix(VOID, SIGNAL, 0.10);
    v.widgets.hovered.bg_fill = mix(VOID, SIGNAL, 0.18);
    v.widgets.active.bg_fill = mix(VOID, SIGNAL, 0.28);
    base_widgets(&mut v, PAPER, SIGNAL);
    v
}

fn chamber_light() -> egui::Visuals {
    let mut v = egui::Visuals::light();
    v.dark_mode = false;
    v.extreme_bg_color = mix(PAPER, VOID, 0.06);
    v.panel_fill = PAPER;
    v.window_fill = egui::Color32::WHITE;
    v.faint_bg_color = mix(PAPER, VOID, 0.04);
    v.widgets.noninteractive.bg_fill = mix(PAPER, VOID, 0.03);
    v.widgets.inactive.bg_fill = mix(PAPER, VOID, 0.06);
    v.widgets.hovered.bg_fill = mix(PAPER, SIGNAL, 0.25);
    v.widgets.active.bg_fill = mix(PAPER, SIGNAL, 0.35);
    base_widgets(&mut v, VOID, mix(VOID, SIGNAL, 0.85));
    // Danger assombri pour rester lisible sur fond clair (surchargable via custom).
    v.warn_fg_color = mix(HYDROGEN, VOID, 0.25);
    v.error_fg_color = mix(HYDROGEN, VOID, 0.25);
    v
}

fn chamber_soft() -> egui::Visuals {
    let mut v = chamber_light();
    let soft_paper = mix(PAPER, VOID, 0.03);
    let soft_fg = mix(VOID, PAPER, 0.10);
    let soft_signal = mix(SIGNAL, VOID, 0.15);
    v.panel_fill = soft_paper;
    v.window_fill = mix(PAPER, VOID, 0.01);
    v.widgets.inactive.bg_fill = mix(soft_paper, VOID, 0.05);
    v.widgets.hovered.bg_fill = mix(soft_paper, soft_signal, 0.20);
    base_widgets(&mut v, soft_fg, soft_signal);
    v.warn_fg_color = mix(HYDROGEN, VOID, 0.25);
    v.error_fg_color = mix(HYDROGEN, VOID, 0.25);
    v
}

fn chamber_high_contrast() -> egui::Visuals {
    let mut v = chamber_dark();
    v.extreme_bg_color = VOID;
    v.panel_fill = VOID;
    v.window_fill = VOID;
    v.override_text_color = Some(PAPER);
    v.widgets.noninteractive.bg_fill = VOID;
    v.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.5_f32, PAPER);
    v.widgets.inactive.bg_fill = VOID;
    v.widgets.inactive.fg_stroke = egui::Stroke::new(1.5_f32, PAPER);
    v.widgets.hovered.bg_fill = mix(VOID, SIGNAL, 0.20);
    v.widgets.hovered.fg_stroke = egui::Stroke::new(2.0_f32, PAPER);
    v.widgets.active.bg_fill = SIGNAL;
    v.widgets.active.fg_stroke = egui::Stroke::new(FOCUS_STROKE_WIDTH, VOID);
    v.selection.bg_fill = SIGNAL;
    v.selection.stroke = egui::Stroke::new(FOCUS_STROKE_WIDTH, PAPER);
    v.window_stroke = egui::Stroke::new(FOCUS_STROKE_WIDTH, SIGNAL);
    apply_focus_ring(&mut v, SIGNAL);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chamber_tokens_are_canonical() {
        assert_eq!(VOID, egui::Color32::from_rgb(7, 11, 20));
        assert_eq!(ICE_TRACK, egui::Color32::from_rgb(94, 231, 255));
        assert_eq!(SIGNAL, egui::Color32::from_rgb(46, 240, 200));
        assert_eq!(HYDROGEN, egui::Color32::from_rgb(255, 90, 72));
        assert_eq!(PAPER, egui::Color32::from_rgb(232, 238, 246));
    }

    #[test]
    fn high_contrast_paper_on_void_meets_aa_body_text() {
        let ratio = contrast_ratio(PAPER, VOID);
        assert!(
            ratio >= 12.0,
            "expected paper-on-void >= 12:1, got {ratio:.2}"
        );
    }

    #[test]
    fn high_contrast_signal_on_void_meets_ui_component_contrast() {
        let ratio = contrast_ratio(SIGNAL, VOID);
        assert!(
            ratio >= 3.0,
            "expected signal-on-void >= 3:1 for UI accents, got {ratio:.2}"
        );
    }

    #[test]
    fn light_void_on_paper_meets_aa_body_text() {
        let ratio = contrast_ratio(VOID, PAPER);
        assert!(
            ratio >= 12.0,
            "expected void-on-paper >= 12:1, got {ratio:.2}"
        );
    }

    #[test]
    fn hit_targets_meet_wcag_minimum() {
        assert!(PRIMARY_MIN_H >= 44.0);
        assert!(ICON_HIT >= 24.0);
        assert!(TOOLBAR_HIT >= 24.0);
        assert!(CONTROL_MIN_H_COMFORTABLE >= 32.0);
    }

    #[test]
    fn theme_colors_follow_each_theme() {
        let custom = crate::prefs::CustomThemePreferences::default();
        let dark = theme_colors("dark", &custom);
        assert_eq!(dark.accent, SIGNAL);
        assert_eq!(dark.danger, HYDROGEN);
        // Light assombrit danger/success/warning pour contraste sur PAPER.
        let light = theme_colors("light", &custom);
        assert_ne!(light.danger, HYDROGEN);
        assert_ne!(light.success, SUCCESS);
        assert_ne!(light.warning, WARNING);
        let hc = theme_colors("high_contrast", &custom);
        assert_eq!(hc.accent, SIGNAL);
    }

    #[test]
    fn custom_theme_overrides_buttons_and_falls_back_on_bad_hex() {
        let mut custom = crate::prefs::CustomThemePreferences::default();
        custom.accent = "#FF0000".into();
        custom.danger = "#00FF00".into();
        custom.success = "#0000FF".into();
        custom.warning = "not-a-hex".into();
        let c = theme_colors("custom", &custom);
        assert_eq!(c.accent, egui::Color32::from_rgb(255, 0, 0));
        assert_eq!(c.danger, egui::Color32::from_rgb(0, 255, 0));
        assert_eq!(c.success, egui::Color32::from_rgb(0, 0, 255));
        // Hex invalide -> fallback canonique, jamais de panique.
        assert_eq!(c.warning, WARNING);
    }
}
