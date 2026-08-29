//! More → Library / Bibliothèque — personal document list (separate from research docs).

use aos_proto::UserLibraryDoc;
use eframe::egui::{self, Ui};

use crate::i18n::UiStrings;

#[derive(Debug, Default)]
pub struct LibraryPanelState {
    pub docs: Vec<UserLibraryDoc>,
}

#[derive(Debug, Default)]
pub struct LibraryPanelActions {
    pub add_clicked: bool,
    pub remove_id: Option<String>,
}

pub fn render(
    ui: &mut Ui,
    t: &UiStrings,
    state: &LibraryPanelState,
    timezone_offset_minutes: i32,
) -> LibraryPanelActions {
    let mut actions = LibraryPanelActions::default();
    ui.heading(t.tab_library);
    ui.label(t.library_hint);
    ui.add_space(6.0);
    if ui.button(t.library_add).clicked() {
        actions.add_clicked = true;
    }
    ui.separator();
    if state.docs.is_empty() {
        ui.weak(t.library_empty);
    } else {
        for doc in &state.docs {
            ui.horizontal(|ui| {
                ui.label(&doc.label);
                if let Some(date) = format_added_date(doc.added_ms, timezone_offset_minutes) {
                    ui.weak(date);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button(t.library_remove).clicked() {
                        actions.remove_id = Some(doc.id.clone());
                    }
                });
            });
            ui.add_space(4.0);
        }
    }
    actions
}

/// Local calendar date (YYYY-MM-DD) from epoch ms — title only in UI, never paths.
fn format_added_date(added_ms: u64, offset_minutes: i32) -> Option<String> {
    if added_ms == 0 {
        return None;
    }
    let local_secs = (added_ms as i64 / 1000) + (offset_minutes as i64 * 60);
    let days = local_secs.div_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    Some(format!("{y:04}-{m:02}-{d:02}"))
}

fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_097 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut y = (yoe + era * 400) as i32;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = ((mp + if mp < 10 { 3 } else { -9 }) % 12 + 1) as u32;
    if m <= 2 {
        y += 1;
    }
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_date_is_calendar_not_path() {
        let date = format_added_date(1_735_689_600_000, 0).expect("date");
        assert!(date.contains('-'));
        assert!(!date.contains('/'));
        assert!(!date.contains('\\'));
        assert!(!date.contains(".pdf"));
    }

    #[test]
    fn zero_added_ms_has_no_date() {
        assert!(format_added_date(0, 0).is_none());
    }
}
