//! Inference studio: live tok/s, TTFT, context fill and NVIDIA GPUs.
//!
//! History is a client-side ring buffer fed by the existing `model.metrics` poll.
//! Expert routing is intentionally absent (no llama.cpp export).

use crate::models_page;
use crate::ui_format::human_bytes;
use crate::{i18n, UiApp};
use aos_proto::{GpuLive, ModelMetrics, SystemMetrics};
use eframe::egui;
use egui_plot::{Line, Plot, PlotPoints};
use std::collections::HashMap;

pub const STUDIO_HISTORY_CAP: usize = 120;

#[derive(Debug, Clone, Default)]
pub struct StudioHistory {
    tok_s: HashMap<String, Vec<f64>>,
    ttft_ms: HashMap<String, Vec<f64>>,
    gpu_util: HashMap<u32, Vec<f32>>,
}

impl StudioHistory {
    pub fn push(&mut self, metrics: &SystemMetrics) {
        for model in &metrics.models {
            self.record_model(&model.model_id, model.last_tok_s, model.last_ttft_ms);
        }
        let live: Vec<u32> = metrics.gpus.iter().map(|gpu| gpu.index).collect();
        self.gpu_util.retain(|index, _| live.contains(index));
        for gpu in &metrics.gpus {
            if let Some(util) = gpu.util_percent {
                push_capped(self.gpu_util.entry(gpu.index).or_default(), util);
            }
        }
    }

    pub fn record_model(&mut self, id: &str, tok_s: Option<f64>, ttft_ms: Option<f64>) {
        if let Some(tok_s) = tok_s {
            push_capped(self.tok_s.entry(id.to_string()).or_default(), tok_s);
        }
        if let Some(ttft_ms) = ttft_ms {
            push_capped(self.ttft_ms.entry(id.to_string()).or_default(), ttft_ms);
        }
    }

    pub fn tok_s(&self, id: &str) -> &[f64] {
        self.tok_s.get(id).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn ttft_ms(&self, id: &str) -> &[f64] {
        self.ttft_ms.get(id).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn gpu_util(&self, index: u32) -> &[f32] {
        self.gpu_util.get(&index).map(Vec::as_slice).unwrap_or(&[])
    }
}

fn push_capped<T>(buf: &mut Vec<T>, value: T) {
    buf.push(value);
    if buf.len() > STUDIO_HISTORY_CAP {
        let drop_n = buf.len() - STUDIO_HISTORY_CAP;
        buf.drain(0..drop_n);
    }
}

const FOCUS_KEYS: [egui::Key; 9] = [
    egui::Key::Num1,
    egui::Key::Num2,
    egui::Key::Num3,
    egui::Key::Num4,
    egui::Key::Num5,
    egui::Key::Num6,
    egui::Key::Num7,
    egui::Key::Num8,
    egui::Key::Num9,
];

impl UiApp {
    pub(crate) fn ui_studio(&mut self, ui: &mut egui::Ui) {
        let t = i18n::strings(&self.prefs.language);
        let french = t.models_tab_installed == "Installés";
        ui.heading(t.studio_heading);
        ui.weak(t.studio_hint);
        ui.add_space(crate::theme::SPACE_UNIT);

        let system = self.metrics.clone();
        let models = system
            .as_ref()
            .map(|metrics| metrics.models.clone())
            .unwrap_or_default();
        let gpus = system
            .as_ref()
            .map(|metrics| metrics.gpus.clone())
            .unwrap_or_default();

        if models.is_empty() {
            ui.weak(t.studio_empty);
        } else {
            self.ui_studio_models(ui, &t, french, &models);
        }

        ui.add_space(crate::theme::SPACE_UNIT * 2.0);
        ui.separator();
        ui.add_space(crate::theme::SPACE_UNIT);
        ui.strong(t.studio_gpu);
        if gpus.is_empty() {
            ui.weak(t.studio_no_gpu);
        } else {
            self.ui_studio_gpus(ui, &t, &gpus);
        }
    }

    fn ui_studio_models(
        &mut self,
        ui: &mut egui::Ui,
        t: &i18n::UiStrings,
        french: bool,
        models: &[ModelMetrics],
    ) {
        let jump = ui.input(|input| {
            if input.modifiers.command || input.modifiers.alt {
                return None;
            }
            FOCUS_KEYS
                .iter()
                .position(|key| input.key_pressed(*key))
                .filter(|index| *index < models.len())
        });
        if let Some(index) = jump {
            self.studio_focus = Some(models[index].model_id.clone());
        } else if self
            .studio_focus
            .as_ref()
            .is_none_or(|id| !models.iter().any(|model| &model.model_id == id))
        {
            self.studio_focus = models.first().map(|model| model.model_id.clone());
        }

        let mut clicked = None;
        ui.horizontal_wrapped(|ui| {
            for (index, model) in models.iter().enumerate() {
                let selected = self.studio_focus.as_deref() == Some(model.model_id.as_str());
                let tok = model
                    .last_tok_s
                    .map(|value| format!("{value:.1}"))
                    .unwrap_or_else(|| "—".into());
                let prefix = if index < 9 {
                    format!("{} ", index + 1)
                } else {
                    String::new()
                };
                let state = models_page::model_state_human(&model.state, french);
                let label = format!(
                    "{prefix}{} · {state} · {tok} {}",
                    model.model_id, t.metrics_tok_s
                );
                if ui.selectable_label(selected, label).clicked() {
                    clicked = Some(model.model_id.clone());
                }
            }
        });
        if let Some(id) = clicked {
            self.studio_focus = Some(id);
        }

        let Some(focus_id) = self.studio_focus.clone() else {
            return;
        };
        let Some(model) = models.iter().find(|model| model.model_id == focus_id) else {
            return;
        };
        let tok_series = self.studio_history.tok_s(&focus_id).to_vec();
        let ttft_series = self.studio_history.ttft_ms(&focus_id).to_vec();

        ui.add_space(crate::theme::SPACE_UNIT);
        ui.horizontal_wrapped(|ui| {
            big_metric(
                ui,
                t.metrics_tok_s,
                &model
                    .last_tok_s
                    .map(|value| format!("{value:.1}"))
                    .unwrap_or_else(|| "—".into()),
            );
            big_metric(
                ui,
                t.metrics_ttft,
                &model
                    .last_ttft_ms
                    .map(|value| format!("{value:.0} ms"))
                    .unwrap_or_else(|| "—".into()),
            );
            if let Some(rate) = model.draft_acceptance_rate {
                big_metric(ui, t.metrics_draft_rate, &format!("{:.0}%", rate * 100.0));
            }
            big_metric(ui, t.metrics_active, &model.active_inferences.to_string());
            big_metric(ui, t.metrics_queued, &model.queued.to_string());
        });

        ui.add_space(crate::theme::SPACE_UNIT);
        let plot_w = ((ui.available_width() - 12.0) / 2.0).max(140.0);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_min_width(plot_w);
                ui.weak(t.studio_history_speed);
                history_plot(ui, &format!("studio_tok_{focus_id}"), &tok_series, plot_w);
            });
            ui.vertical(|ui| {
                ui.set_min_width(plot_w);
                ui.weak(t.studio_history_ttft);
                history_plot(ui, &format!("studio_ttft_{focus_id}"), &ttft_series, plot_w);
            });
        });

        ui.add_space(crate::theme::SPACE_UNIT);
        ui.strong(t.studio_context);
        let label = context_label(t, model.ctx_used, model.n_ctx);
        let fraction = match (model.ctx_used, model.n_ctx) {
            (Some(used), Some(window)) if window > 0 => {
                (used as f32 / window as f32).clamp(0.0, 1.0)
            }
            _ => 0.0,
        };
        ui.add(egui::ProgressBar::new(fraction).text(label));
        ui.horizontal_wrapped(|ui| {
            metric_line(ui, t.metrics_vram, human_bytes(model.vram_bytes));
            metric_line(ui, t.metrics_ram, human_bytes(model.ram_bytes));
            metric_line(ui, t.metrics_disk, human_bytes(model.disk_bytes));
        });
    }

    fn ui_studio_gpus(&self, ui: &mut egui::Ui, t: &i18n::UiStrings, gpus: &[GpuLive]) {
        for gpu in gpus {
            let util_series: Vec<f64> = self
                .studio_history
                .gpu_util(gpu.index)
                .iter()
                .map(|value| f64::from(*value))
                .collect();
            ui.add_space(crate::theme::SPACE_UNIT);
            egui::Frame::group(ui.style())
                .inner_margin(egui::Margin::same(10))
                .show(ui, |ui| {
                    ui.strong(format!("{} · {}", gpu.index, gpu.name));
                    if let Some(util) = gpu.util_percent {
                        ui.add(
                            egui::ProgressBar::new((util / 100.0).clamp(0.0, 1.0))
                                .text(format!("{} {:.0}%", t.studio_util, util)),
                        );
                    }
                    if let (Some(used), Some(total)) = (gpu.vram_used_mib, gpu.vram_total_mib) {
                        let fraction = if total == 0 {
                            0.0
                        } else {
                            (used as f32 / total as f32).clamp(0.0, 1.0)
                        };
                        ui.add(
                            egui::ProgressBar::new(fraction)
                                .text(format!("{} {used} / {total} MiB", t.metrics_vram)),
                        );
                    }
                    ui.horizontal_wrapped(|ui| {
                        if let Some(temp) = gpu.temp_c {
                            metric_line(ui, t.studio_temp, format!("{temp:.0} °C"));
                        }
                        if let Some(power) = gpu.power_w {
                            metric_line(ui, t.studio_power, format!("{power:.0} W"));
                        }
                    });
                    if !util_series.is_empty() {
                        let width = ui.available_width().max(160.0);
                        history_plot(
                            ui,
                            &format!("studio_gpu_{}", gpu.index),
                            &util_series,
                            width,
                        );
                    }
                });
        }
    }
}

fn context_label(t: &i18n::UiStrings, used: Option<u32>, window: Option<u32>) -> String {
    let used = used
        .map(|value| value.to_string())
        .unwrap_or_else(|| "—".into());
    let window = window
        .map(|value| value.to_string())
        .unwrap_or_else(|| "—".into());
    t.studio_context_of
        .replace("{used}", &used)
        .replace("{window}", &window)
}

fn big_metric(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.vertical(|ui| {
        ui.weak(label);
        ui.label(egui::RichText::new(value).size(28.0).strong());
    });
    ui.add_space(12.0);
}

fn metric_line(ui: &mut egui::Ui, label: &str, value: String) {
    ui.horizontal(|ui| {
        ui.weak(format!("{label}:"));
        ui.monospace(value);
    });
}

fn history_plot(ui: &mut egui::Ui, id: &str, series: &[f64], width: f32) {
    if series.is_empty() {
        ui.weak("—");
        return;
    }
    Plot::new(id)
        .height(140.0)
        .width(width)
        .allow_drag(false)
        .allow_zoom(false)
        .allow_scroll(false)
        .show_axes([false, true])
        .show(ui, |plot_ui| {
            plot_ui.line(Line::new(PlotPoints::from_iter(
                series
                    .iter()
                    .enumerate()
                    .map(|(index, value)| [index as f64, *value]),
            )));
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_buffer_caps_and_keeps_newest() {
        let mut history = StudioHistory::default();
        for i in 0..200 {
            history.record_model("m", Some(i as f64), None);
        }
        let series = history.tok_s("m");
        assert_eq!(series.len(), STUDIO_HISTORY_CAP);
        assert_eq!(series[0], 80.0);
        assert_eq!(series[series.len() - 1], 199.0);
        assert!(history.ttft_ms("m").is_empty());
    }
}
