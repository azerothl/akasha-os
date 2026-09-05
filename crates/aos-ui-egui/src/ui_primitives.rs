//! Port minimal opensourceui.in -> egui (MIT, bidyut10/opensourceui).
//!
//! Ne copie aucun code React : seuls les comportements sont réimplémentés
//! avec les tokens `crate::theme` :
//! - `3d-button / tactile` -> `primary_button` 44px
//! - `search-input` -> `search_field` avec clear + Escape
//! - `toast-notification` -> `Toasts` bottom-right auto-dismiss
//! - `slide-to-confirm / hold-to-delete` -> `danger_confirm_button`
//!   (two-step arm+confirm : drag impraticable au clavier en egui,
//!   même garantie anti-clic accidentel).

use eframe::egui;
use std::time::{Duration, Instant};

/// CTA principal 44px (opensourceui `tactile 3D`). Un seul par vue.
#[allow(dead_code)]
pub fn primary_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let h = crate::theme::PRIMARY_MIN_H;
    ui.add_sized(
        egui::vec2(ui.available_width().min(320.0), h),
        egui::Button::new(label).corner_radius(crate::theme::RADIUS_MD),
    )
}

/// Bouton danger two-step : 1er clic arme ("Supprimer ?"), 2e clic confirme.
/// `id_salt` doit être unique par ligne (ex. model id). L'armement expire
/// après 4s. Équivalent clavier-accessible du `slide-to-confirm`.
pub fn danger_confirm_button(
    ui: &mut egui::Ui,
    id_salt: impl std::hash::Hash,
    label: &str,
    armed_label: &str,
) -> bool {
    let id = ui.id().with(id_salt);
    let armed_at: Option<Instant> = ui.memory(|m| m.data.get_temp(id));
    let armed = armed_at.is_some_and(|t| t.elapsed() < Duration::from_secs(4));
    let text = if armed { armed_label } else { label };
    let mut btn = egui::Button::new(text).corner_radius(crate::theme::RADIUS_SM);
    if armed {
        btn = btn.fill(crate::theme::HYDROGEN);
    }
    let resp = ui.add_enabled(true, btn);
    if resp.clicked() {
        if armed {
            ui.memory_mut(|m| m.data.remove::<Instant>(id));
            return true;
        }
        ui.memory_mut(|m| m.data.insert_temp(id, Instant::now()));
    }
    false
}

/// Champ recherche style `search-input` : icône loupe (texte), clear 28px,
/// `Escape` vide sans fermer la palette parente (le parent gère Escape).
pub fn search_field(ui: &mut egui::Ui, text: &mut String, hint: &str) -> egui::Response {
    ui.horizontal(|ui| {
        ui.weak("Search");
        let resp = ui.add_sized(
            egui::vec2(
                (ui.available_width() - crate::theme::ICON_HIT - 8.0).max(120.0),
                36.0,
            ),
            egui::TextEdit::singleline(text).hint_text(hint),
        );
        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            text.clear();
            ui.memory_mut(|m| m.request_focus(resp.id));
        }
        if !text.is_empty() {
            // Hit 28px, glyphe 18px via theme — même grille que icons.rs.
            if ui
                .add_sized(
                    egui::Vec2::splat(crate::theme::ICON_HIT),
                    egui::Button::new("x").corner_radius(crate::theme::RADIUS_SM),
                )
                .on_hover_text("Clear")
                .clicked()
            {
                text.clear();
            }
        } else {
            ui.add_space(crate::theme::ICON_HIT);
        }
        resp
    })
    .inner
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    #[allow(dead_code)]
    Info,
    Success,
    Error,
}

#[derive(Debug, Clone)]
struct ToastItem {
    kind: ToastKind,
    msg: String,
    created: Instant,
}

#[derive(Debug, Default)]
pub struct Toasts {
    items: Vec<ToastItem>,
}

impl Toasts {
    pub fn push(&mut self, kind: ToastKind, msg: impl Into<String>) {
        let msg = msg.into();
        if msg.is_empty() {
            return;
        }
        // Évite le spam : déduplique le dernier identique de moins de 2s.
        if let Some(last) = self.items.last() {
            if last.msg == msg && last.created.elapsed() < Duration::from_secs(2) {
                return;
            }
        }
        self.items.push(ToastItem {
            kind,
            msg,
            created: Instant::now(),
        });
        self.items.truncate(3);
    }

    #[allow(dead_code)]
    pub fn push_info(&mut self, msg: impl Into<String>) {
        self.push(ToastKind::Info, msg);
    }
    pub fn push_success(&mut self, msg: impl Into<String>) {
        self.push(ToastKind::Success, msg);
    }
    pub fn push_error(&mut self, msg: impl Into<String>) {
        self.push(ToastKind::Error, msg);
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        self.items
            .retain(|t| t.created.elapsed() < Duration::from_millis(4500));
        if self.items.is_empty() {
            return;
        }
        egui::Area::new(egui::Id::new("aos_toasts"))
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-12.0, -40.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    let mut dismiss = Vec::new();
                    for (idx, toast) in self.items.iter().enumerate() {
                        let (accent, label) = match toast.kind {
                            ToastKind::Info => (crate::theme::SIGNAL, "i"),
                            ToastKind::Success => (crate::theme::SUCCESS, "OK"),
                            ToastKind::Error => (crate::theme::HYDROGEN, "!"),
                        };
                        egui::Frame::group(ui.style())
                            .corner_radius(crate::theme::RADIUS_LG)
                            .stroke(egui::Stroke::new(1.0, accent))
                            .inner_margin(egui::Margin::same(10))
                            .show(ui, |ui| {
                                ui.set_max_width(340.0);
                                ui.horizontal(|ui| {
                                    ui.colored_label(accent, label);
                                    ui.label(&toast.msg);
                                    if ui.small_button("x").clicked() {
                                        dismiss.push(idx);
                                    }
                                });
                            });
                        ui.add_space(6.0);
                    }
                    for idx in dismiss.into_iter().rev() {
                        self.items.remove(idx);
                    }
                });
            });
    }
}

/// Filtre insensible à la casse pour la palette Spotlight (`spotlight-bar`).
/// `query` vide -> tout. Normalise via `to_lowercase` (accents FR ok).
pub fn filter_labels<'a>(query: &str, labels: &'a [&'a str]) -> Vec<usize> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return (0..labels.len()).collect();
    }
    labels
        .iter()
        .enumerate()
        .filter(|(_, l)| l.to_lowercase().contains(q.as_str()))
        .map(|(i, _)| i)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_returns_all() {
        let labels = ["Chat", "Agents", "Models"];
        assert_eq!(filter_labels("", &labels), vec![0, 1, 2]);
    }

    #[test]
    fn filter_is_case_insensitive_and_accent_aware() {
        let labels = ["Modèles", "Mémoire", "Chat"];
        assert_eq!(filter_labels("modele", &labels), Vec::<usize>::new());
        // `modèle` matche `modèles` en substring, ` MEMOIRE ` sans accent ne matche pas `mémoire`.
        assert_eq!(filter_labels("modèle", &labels), vec![0]);
        assert_eq!(filter_labels("CHAT", &labels), vec![2]);
    }

    #[test]
    fn toasts_dedupe_and_cap() {
        let mut t = Toasts::default();
        t.push_error("boom");
        t.push_error("boom");
        assert_eq!(t.items.len(), 1);
        t.push_info("a");
        t.push_info("b");
        t.push_info("c");
        assert!(t.items.len() <= 3);
    }
}
