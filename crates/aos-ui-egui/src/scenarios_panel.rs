//! Guided cohort scenarios tab (P09.7 i18n).

use crate::i18n::UiStrings;
use eframe::egui;

pub struct ScenarioFlags {
    pub chat: bool,
    pub note_human: bool,
    pub note_agent: bool,
    pub confirm: bool,
    pub audit: bool,
    pub module_agent: bool,
}

pub fn ui(
    ui: &mut egui::Ui,
    t: &UiStrings,
    flags: &mut ScenarioFlags,
    on_launch_module: impl FnOnce(),
    on_test_confirm: impl FnOnce(),
) {
    ui.heading(t.scen_heading);
    ui.label(t.scen_blurb);
    ui.checkbox(&mut flags.chat, t.scen_1_chat);
    ui.checkbox(&mut flags.note_human, t.scen_2_note_human);
    ui.checkbox(&mut flags.note_agent, t.scen_3_note_agent);
    ui.checkbox(&mut flags.confirm, t.scen_4_confirm);
    ui.checkbox(&mut flags.audit, t.scen_5_audit);
    ui.checkbox(&mut flags.module_agent, t.scen_module_agent);
    ui.weak(t.scen_module_agent_hint);
    if ui.button(t.scen_module_agent_launch).clicked() {
        on_launch_module();
    }
    ui.label(t.scen_pc69);
    ui.separator();
    let done = [
        flags.chat,
        flags.note_human,
        flags.note_agent,
        flags.confirm,
        flags.audit,
        flags.module_agent,
    ]
    .iter()
    .filter(|x| **x)
    .count();
    ui.label(t.scen_progress.replace("{done}", &done.to_string()));
    if done == 6 {
        ui.colored_label(egui::Color32::LIGHT_GREEN, t.scen_done);
    }
    if ui.button(t.scen_test_confirm).clicked() {
        on_test_confirm();
    }
}
