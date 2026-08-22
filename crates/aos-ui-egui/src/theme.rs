//! Orrery design tokens mapped to egui Visuals (see docs/UI.md, docs/DESIGN.md).

use eframe::egui;

const VOID: egui::Color32 = egui::Color32::from_rgb(7, 11, 20);
const SIGNAL: egui::Color32 = egui::Color32::from_rgb(62, 224, 196);
const HYDROGEN: egui::Color32 = egui::Color32::from_rgb(232, 93, 76);
const PAPER: egui::Color32 = egui::Color32::from_rgb(232, 238, 246);

fn mix(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let t = t.clamp(0.0, 1.0);
    let inv = 1.0 - t;
    egui::Color32::from_rgb(
        (f32::from(a.r()) * inv + f32::from(b.r()) * t) as u8,
        (f32::from(a.g()) * inv + f32::from(b.g()) * t) as u8,
        (f32::from(a.b()) * inv + f32::from(b.b()) * t) as u8,
    )
}

fn base_widgets(v: &mut egui::Visuals, fg: egui::Color32, accent: egui::Color32) {
    v.override_text_color = Some(fg);
    v.selection.bg_fill = accent;
    v.selection.stroke = egui::Stroke::new(1.0_f32, VOID);
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
    v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, accent);
    v.widgets.active.bg_stroke = egui::Stroke::new(1.5_f32, accent);
    v.widgets.active.fg_stroke = egui::Stroke::new(1.5_f32, VOID);
}

pub fn apply_theme(ctx: &egui::Context, theme: &str) {
    let visuals = match theme {
        "light" => orrery_light(),
        "soft" => orrery_soft(),
        "high_contrast" => orrery_high_contrast(),
        _ => orrery_dark(),
    };
    ctx.set_visuals(visuals);
}

fn orrery_dark() -> egui::Visuals {
    let mut v = egui::Visuals::dark();
    v.dark_mode = true;
    v.extreme_bg_color = VOID;
    v.panel_fill = VOID;
    v.window_fill = mix(VOID, PAPER, 0.04);
    v.faint_bg_color = mix(VOID, SIGNAL, 0.08);
    v.widgets.noninteractive.bg_fill = mix(VOID, SIGNAL, 0.06);
    v.widgets.inactive.bg_fill = mix(VOID, SIGNAL, 0.10);
    v.widgets.hovered.bg_fill = mix(VOID, SIGNAL, 0.18);
    v.widgets.active.bg_fill = SIGNAL;
    base_widgets(&mut v, PAPER, SIGNAL);
    v
}

fn orrery_light() -> egui::Visuals {
    let mut v = egui::Visuals::light();
    v.dark_mode = false;
    v.extreme_bg_color = mix(PAPER, VOID, 0.06);
    v.panel_fill = PAPER;
    v.window_fill = egui::Color32::WHITE;
    v.faint_bg_color = mix(PAPER, VOID, 0.04);
    v.widgets.noninteractive.bg_fill = mix(PAPER, VOID, 0.03);
    v.widgets.inactive.bg_fill = mix(PAPER, VOID, 0.06);
    v.widgets.hovered.bg_fill = mix(PAPER, SIGNAL, 0.25);
    v.widgets.active.bg_fill = SIGNAL;
    base_widgets(&mut v, VOID, mix(VOID, SIGNAL, 0.85));
    v
}

fn orrery_soft() -> egui::Visuals {
    let mut v = orrery_light();
    let soft_paper = mix(PAPER, VOID, 0.03);
    let soft_fg = mix(VOID, PAPER, 0.10);
    let soft_signal = mix(SIGNAL, VOID, 0.15);
    v.panel_fill = soft_paper;
    v.window_fill = mix(PAPER, VOID, 0.01);
    v.widgets.inactive.bg_fill = mix(soft_paper, VOID, 0.05);
    v.widgets.hovered.bg_fill = mix(soft_paper, soft_signal, 0.20);
    base_widgets(&mut v, soft_fg, soft_signal);
    v
}

fn orrery_high_contrast() -> egui::Visuals {
    let mut v = orrery_dark();
    v.override_text_color = Some(PAPER);
    v.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.5_f32, PAPER);
    v.widgets.inactive.fg_stroke = egui::Stroke::new(1.5_f32, PAPER);
    v.widgets.hovered.fg_stroke = egui::Stroke::new(2.0_f32, PAPER);
    v.widgets.active.fg_stroke = egui::Stroke::new(2.0_f32, VOID);
    v.widgets.active.bg_fill = PAPER;
    v.selection.bg_fill = SIGNAL;
    v.selection.stroke = egui::Stroke::new(2.0_f32, PAPER);
    v.window_stroke = egui::Stroke::new(2.0_f32, SIGNAL);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orrery_tokens_are_canonical() {
        assert_eq!(VOID, egui::Color32::from_rgb(7, 11, 20));
        assert_eq!(SIGNAL, egui::Color32::from_rgb(62, 224, 196));
        assert_eq!(HYDROGEN, egui::Color32::from_rgb(232, 93, 76));
        assert_eq!(PAPER, egui::Color32::from_rgb(232, 238, 246));
    }
}
