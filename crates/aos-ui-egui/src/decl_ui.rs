//! Host-rendered declarative module UI (E15 / Preview 0.7).

use aos_proto::decl_ui::{resolve_row_args, DeclUiDocument, DeclUiRowAction, DeclUiWidget};
use aos_proto::rich_decl_ui::{eval_predicate, resolve_action_input, RichAction, RichJobHandle};
use aos_proto::ModuleTool;
use crate::rich_decl::{
    init_state_from_schema, ImageViewInteractionState, JobProgressThrottle, RichDeclSubscriptions,
};
use eframe::egui::{self, Ui};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use egui_plot::{Bar, BarChart, Line, Plot, PlotPoints, Points};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct DeclUiServiceAction {
    pub action_id: String,
    pub service: Option<String>,
    pub tool: Option<String>,
    pub input: Value,
    pub refresh_binds: Vec<String>,
    pub subscription_id: Option<String>,
}

#[derive(Debug, Default)]
pub struct DeclUiActions {
    pub refresh: bool,
    pub invoke: Option<DeclUiInvokeAction>,
    pub service_action: Option<DeclUiServiceAction>,
    pub cancel_job: Option<(String, String)>,
    pub local_patch: HashMap<String, Value>,
}

#[derive(Debug, Clone)]
pub struct DeclUiInvokeAction {
    pub tool: String,
    pub args: Value,
    pub refresh_binds: Vec<String>,
    pub clear_form_keys: Vec<String>,
}

#[derive(Debug, Default)]
pub struct DeclUiPanelState {
    #[allow(dead_code)]
    pub module: String,
    pub document: Option<DeclUiDocument>,
    pub contract: u32,
    pub error: String,
    pub bind_cache: HashMap<String, Value>,
    pub binding_cache: HashMap<String, Value>,
    pub local_state: HashMap<String, Value>,
    pub document_state: HashMap<String, Value>,
    pub subscriptions: RichDeclSubscriptions,
    pub image_views: HashMap<String, ImageViewInteractionState>,
    pub job_throttle: JobProgressThrottle,
    pub form_fields: HashMap<String, String>,
    pub status: String,
    pub tool_schemas: HashMap<String, Value>,
    pub pending_invoke: bool,
    pub pending_refresh_binds: Vec<String>,
    pub pending_clear_form_keys: Vec<String>,
}

impl DeclUiPanelState {
    pub fn new(module: impl Into<String>) -> Self {
        Self {
            module: module.into(),
            ..Default::default()
        }
    }

    pub fn set_document(&mut self, doc: DeclUiDocument) {
        self.error.clear();
        self.contract = doc.contract.unwrap_or(1);
        if let Some(state) = &doc.state {
            let (local, document) = init_state_from_schema(state);
            self.local_state = local;
            self.document_state = document;
        }
        self.subscriptions.clear();
        for sub in &doc.subscriptions {
            self.subscriptions.register(&sub.id);
        }
        self.document = Some(doc);
    }

    pub fn close(&mut self) {
        self.subscriptions.clear();
        self.job_throttle.clear();
        self.image_views.clear();
        self.binding_cache.clear();
    }

    pub fn set_binding_result(&mut self, binding_id: &str, result: Value) {
        self.binding_cache.insert(binding_id.to_string(), result);
    }

    pub fn set_job_update(&mut self, subscription_id: &str, job: RichJobHandle) {
        if self.job_throttle.allow(job.job_id.as_deref().unwrap_or(subscription_id)) {
            self.subscriptions.set_job(subscription_id, job);
        }
    }

    pub fn set_error(&mut self, msg: impl Into<String>) {
        self.document = None;
        self.error = msg.into();
    }

    pub fn set_bind_result(&mut self, tool: &str, result: Value) {
        self.bind_cache.insert(tool.to_string(), result);
    }

    pub fn set_pending_invoke(&mut self, pending: bool) {
        self.pending_invoke = pending;
    }

    pub fn clear_form_keys(&mut self, keys: &[String]) {
        for key in keys {
            self.form_fields.remove(key);
        }
    }

    pub fn tools_to_bind(&self) -> Vec<String> {
        self.document
            .as_ref()
            .map(|d| d.bind_tools())
            .unwrap_or_default()
    }

    pub fn bindings_to_fetch(&self) -> Vec<(String, String)> {
        self.document
            .as_ref()
            .map(|d| {
                d.bindings
                    .iter()
                    .map(|b| (b.id.clone(), b.tool.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn ui(
        &mut self,
        ui: &mut Ui,
        md_cache: &mut CommonMarkCache,
        refresh_label: &str,
        language: &str,
    ) -> DeclUiActions {
        let mut actions = DeclUiActions::default();
        if !self.error.is_empty() {
            ui.colored_label(egui::Color32::RED, &self.error);
            if ui.button(refresh_label).clicked() {
                actions.refresh = true;
            }
            return actions;
        }
        let Some(doc) = self.document.clone() else {
            ui.weak("…");
            return actions;
        };
        let heading = doc.chrome_title(language);
        ui.horizontal(|ui| {
            ui.heading(&heading);
            if ui.button(refresh_label).clicked() {
                actions.refresh = true;
            }
        });
        if !self.status.is_empty() {
            ui.weak(&self.status);
        }
        ui.separator();
        if let Some(root) = doc.root.children.clone() {
            for child in root {
                Self::render_widget(
                    ui,
                    md_cache,
                    &child,
                    &doc,
                    language,
                    &self.bind_cache,
                    &self.binding_cache,
                    &self.local_state,
                    &self.document_state,
                    &self.subscriptions,
                    &mut self.image_views,
                    &mut self.form_fields,
                    &self.tool_schemas,
                    self.pending_invoke,
                    &mut actions,
                );
            }
        } else {
            Self::render_widget(
                ui,
                md_cache,
                &doc.root,
                &doc,
                language,
                &self.bind_cache,
                &self.binding_cache,
                &self.local_state,
                &self.document_state,
                &self.subscriptions,
                &mut self.image_views,
                &mut self.form_fields,
                &self.tool_schemas,
                self.pending_invoke,
                &mut actions,
            );
        }
        actions
    }

    fn render_widget(
        ui: &mut Ui,
        md_cache: &mut CommonMarkCache,
        w: &DeclUiWidget,
        doc: &DeclUiDocument,
        language: &str,
        cache: &HashMap<String, Value>,
        binding_cache: &HashMap<String, Value>,
        local_state: &HashMap<String, Value>,
        document_state: &HashMap<String, Value>,
        subscriptions: &RichDeclSubscriptions,
        image_views: &mut HashMap<String, ImageViewInteractionState>,
        form_fields: &mut HashMap<String, String>,
        tool_schemas: &HashMap<String, Value>,
        pending_invoke: bool,
        actions: &mut DeclUiActions,
    ) {
        if let Some(pred) = &w.visible {
            if !eval_predicate(pred, local_state, document_state) {
                return;
            }
        }
        let enabled = w
            .enabled
            .as_ref()
            .map(|p| eval_predicate(p, local_state, document_state))
            .unwrap_or(true);
        match w.kind.as_str() {
            "column" => {
                let h = ui.available_height();
                egui::ScrollArea::vertical()
                    .id_salt("decl_ui_column")
                    .max_height(h.max(120.0))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.vertical(|ui| {
                            if let Some(children) = &w.children {
                                for c in children {
                                    Self::render_widget(
                                        ui,
                                        md_cache,
                                        c,
                                        doc,
                                        language,
                                        cache,
                                        binding_cache,
                                        local_state,
                                        document_state,
                                        subscriptions,
                                        image_views,
                                        form_fields,
                                        tool_schemas,
                                        pending_invoke,
                                        actions,
                                    );
                                }
                            }
                        });
                    });
            }
            "row" => {
                ui.horizontal(|ui| {
                    if let Some(children) = &w.children {
                        for c in children {
                            Self::render_widget(
                                ui,
                                md_cache,
                                c,
                                doc,
                                language,
                                cache,
                                binding_cache,
                                local_state,
                                document_state,
                                subscriptions,
                                image_views,
                                form_fields,
                                tool_schemas,
                                pending_invoke,
                                actions,
                            );
                        }
                    }
                });
            }
            "heading" => {
                if let Some(t) = widget_text(w, doc, language) {
                    ui.heading(t);
                }
            }
            "text" => {
                if let Some(t) = widget_text(w, doc, language) {
                    ui.label(t);
                }
            }
            "markdown" => {
                if let Some(t) = widget_text(w, doc, language) {
                    CommonMarkViewer::new().show(ui, md_cache, &t);
                }
            }
            "empty_state" => {
                if let Some(bind) = &w.bind {
                    let val = resolve_bind(cache, bind, w.source.as_deref());
                    if bind_rows(&val).is_empty() {
                        if let Some(t) = widget_text(w, doc, language) {
                            ui.weak(t);
                        }
                    }
                }
            }
            "count_label" => {
                if let Some(bind) = &w.bind {
                    let val = resolve_bind(cache, bind, w.source.as_deref());
                    let n = bind_rows(&val).len();
                    if let Some(tpl) = widget_text(w, doc, language) {
                        ui.weak(tpl.replace("{n}", &n.to_string()));
                    }
                }
            }
            "stat_row" => {
                if let Some(bind) = &w.bind {
                    let val = resolve_bind(cache, bind, w.source.as_deref());
                    ui.horizontal_wrapped(|ui| {
                        render_stats(ui, &val, w.items.as_deref());
                    });
                }
            }
            "table" => {
                if let Some(bind) = &w.bind {
                    let val = resolve_bind(cache, bind, w.source.as_deref());
                    render_table(
                        ui,
                        &val,
                        w.columns.as_deref(),
                        w.hide_headers.unwrap_or(false),
                        w.row_actions.as_deref(),
                        doc,
                        language,
                        tool_schemas,
                        pending_invoke,
                        actions,
                    );
                }
            }
            "line_chart" => {
                if let Some(bind) = &w.bind {
                    let val = resolve_bind(cache, bind, w.source.as_deref());
                    render_line_chart(ui, &val, w.series.as_deref());
                }
            }
            "bar_chart" => {
                if let Some(bind) = &w.bind {
                    let val = resolve_bind(cache, bind, w.source.as_deref());
                    render_bar_chart(ui, &val, w.series.as_deref());
                }
            }
            "pie" => {
                if let Some(bind) = &w.bind {
                    let val = resolve_bind(cache, bind, w.source.as_deref());
                    render_pie(ui, &val);
                }
            }
            "scatter" => {
                if let Some(bind) = &w.bind {
                    let val = resolve_bind(cache, bind, w.source.as_deref());
                    render_scatter(ui, &val, w.series.as_deref());
                }
            }
            "select" => {
                render_choice(ui, w, form_fields, false);
            }
            "radio" => {
                render_choice(ui, w, form_fields, true);
            }
            "checkbox" => {
                let key = w
                    .label
                    .clone()
                    .or_else(|| w.text.clone())
                    .unwrap_or_else(|| "flag".into());
                let mut on = form_fields.get(&key).map(|s| s == "true").unwrap_or(false);
                if ui.checkbox(&mut on, &key).changed() {
                    form_fields.insert(key, if on { "true".into() } else { "false".into() });
                }
            }
            "textarea" => {
                if let Some(state_key) = &w.state_key {
                    let label = widget_text(w, doc, language)
                        .unwrap_or_else(|| state_key.clone());
                    let mut text = local_state
                        .get(state_key)
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    ui.add_enabled_ui(enabled, |ui| {
                        ui.label(label);
                        if ui.text_edit_multiline(&mut text).changed() {
                            actions
                                .local_patch
                                .insert(state_key.clone(), Value::String(text));
                        }
                    });
                } else {
                    let key = w.label.clone().unwrap_or_else(|| "text".into());
                    form_fields.entry(key.clone()).or_default();
                    ui.label(&key);
                    ui.text_edit_multiline(form_fields.get_mut(&key).unwrap());
                }
            }
            "image" => {
                let path = media_path(w, cache);
                ui.label(format!("image: {path}"));
                if let Some(tex) = try_load_png(ui.ctx(), &path) {
                    ui.image(&tex);
                }
            }
            "audio" => {
                let path = media_path(w, cache);
                ui.horizontal(|ui| {
                    ui.label(format!("audio: {path}"));
                    if ui.button("Play").clicked() {
                        let _ = open_host_path(&path);
                    }
                });
            }
            "button" => {
                let label = widget_text(w, doc, language)
                    .or_else(|| w.label.clone())
                    .or_else(|| w.text.clone())
                    .unwrap_or_else(|| "Run".into());
                let can_run = enabled && !pending_invoke && actions.invoke.is_none();
                if ui.add_enabled(can_run, egui::Button::new(label)).clicked() {
                    if let Some(action_id) = &w.action {
                        if let Some(action) = doc.actions.iter().find(|a| &a.id == action_id) {
                            queue_service_action(
                                actions,
                                action,
                                local_state,
                                document_state,
                                doc,
                            );
                        }
                    } else if let Some(tool) = &w.tool {
                        queue_invoke(
                            actions,
                            tool,
                            w.args
                                .clone()
                                .unwrap_or_else(|| Value::Object(Default::default())),
                            w.refresh_binds.clone().unwrap_or_default(),
                            Vec::new(),
                            tool_schemas,
                        );
                    }
                }
            }
            "form" => {
                if let Some(tool) = &w.tool {
                    let schema = tool_schemas
                        .get(tool)
                        .cloned()
                        .or_else(|| w.args.clone())
                        .unwrap_or_else(|| serde_json::json!({"type":"object","properties":{}}));
                    let fields = schema_fields(&schema, doc, language);
                    let inline = w.prefix_label_key.is_some();
                    ui.group(|ui| {
                        if inline {
                            ui.horizontal(|ui| {
                                if let Some(prefix) = widget_text_from_key(
                                    w.prefix_label_key.as_deref(),
                                    doc,
                                    language,
                                ) {
                                    ui.label(prefix);
                                }
                                render_form_fields(ui, &fields, form_fields, inline);
                                let submit = widget_text(w, doc, language)
                                    .or_else(|| w.label.clone())
                                    .unwrap_or_else(|| "Submit".into());
                                let enabled = !pending_invoke && actions.invoke.is_none();
                                if ui.add_enabled(enabled, egui::Button::new(submit)).clicked() {
                                    submit_decl_form(
                                        actions,
                                        tool,
                                        &fields,
                                        form_fields,
                                        w.refresh_binds.clone().unwrap_or_default(),
                                        tool_schemas,
                                    );
                                }
                            });
                        } else {
                            render_form_fields(ui, &fields, form_fields, inline);
                            let submit = widget_text(w, doc, language)
                                .or_else(|| w.label.clone())
                                .unwrap_or_else(|| "Submit".into());
                            let enabled = !pending_invoke && actions.invoke.is_none();
                            if ui.add_enabled(enabled, egui::Button::new(submit)).clicked() {
                                submit_decl_form(
                                    actions,
                                    tool,
                                    &fields,
                                    form_fields,
                                    w.refresh_binds.clone().unwrap_or_default(),
                                    tool_schemas,
                                );
                            }
                        }
                    });
                }
            }
            "scroll" => {
                egui::ScrollArea::vertical()
                    .id_salt(format!("decl_scroll_{}", w.label_key.as_deref().unwrap_or("")))
                    .show(ui, |ui| {
                        if let Some(children) = &w.children {
                            for c in children {
                                Self::render_widget(
                                    ui,
                                    md_cache,
                                    c,
                                    doc,
                                    language,
                                    cache,
                                    binding_cache,
                                    local_state,
                                    document_state,
                                    subscriptions,
                                    image_views,
                                    form_fields,
                                    tool_schemas,
                                    pending_invoke,
                                    actions,
                                );
                            }
                        }
                    });
            }
            "split" => {
                let ratio = w.split_ratio.unwrap_or(0.5).clamp(0.1, 0.9);
                if let Some(children) = &w.children {
                    if children.len() == 2 {
                        ui.horizontal(|ui| {
                            let w_left = ui.available_width() * ratio;
                            ui.allocate_ui_with_layout(
                                egui::vec2(w_left, ui.available_height()),
                                egui::Layout::top_down(egui::Align::LEFT),
                                |ui| {
                                    Self::render_widget(
                                        ui,
                                        md_cache,
                                        &children[0],
                                        doc,
                                        language,
                                        cache,
                                        binding_cache,
                                        local_state,
                                        document_state,
                                        subscriptions,
                                        image_views,
                                        form_fields,
                                        tool_schemas,
                                        pending_invoke,
                                        actions,
                                    );
                                },
                            );
                            Self::render_widget(
                                ui,
                                md_cache,
                                &children[1],
                                doc,
                                language,
                                cache,
                                binding_cache,
                                local_state,
                                document_state,
                                subscriptions,
                                image_views,
                                form_fields,
                                tool_schemas,
                                pending_invoke,
                                actions,
                            );
                        });
                    }
                }
            }
            "tabs" => {
                if let Some(tabs) = &w.tabs {
                    let labels: Vec<String> = tabs
                        .iter()
                        .enumerate()
                        .map(|(i, tab)| {
                            tab.label
                                .clone()
                                .or_else(|| {
                                    widget_text_from_key(tab.label_key.as_deref(), doc, language)
                                })
                                .unwrap_or_else(|| format!("Tab {}", i + 1))
                        })
                        .collect();
                    let mut selected = 0usize;
                    ui.horizontal(|ui| {
                        for (i, label) in labels.iter().enumerate() {
                            if ui.selectable_label(selected == i, label).clicked() {
                                selected = i;
                            }
                        }
                    });
                    ui.separator();
                    if let Some(tab) = tabs.get(selected) {
                        if let Some(content) = &tab.content {
                            Self::render_widget(
                                ui,
                                md_cache,
                                content,
                                doc,
                                language,
                                cache,
                                binding_cache,
                                local_state,
                                document_state,
                                subscriptions,
                                image_views,
                                form_fields,
                                tool_schemas,
                                pending_invoke,
                                actions,
                            );
                        }
                    }
                }
            }
            "spacer" => {
                let h = w.size.unwrap_or(8.0);
                ui.add_space(h);
            }
            "slider" => {
                if let Some(key) = &w.state_key {
                    let min = w.min.unwrap_or(0.0) as f32;
                    let max = w.max.unwrap_or(100.0) as f32;
                    let mut val = local_state
                        .get(key)
                        .and_then(|v| v.as_f64())
                        .unwrap_or(min as f64) as f32;
                    let label = widget_text(w, doc, language).unwrap_or_else(|| key.clone());
                    ui.add_enabled_ui(enabled, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(label);
                            if ui.add(egui::Slider::new(&mut val, min..=max)).changed() {
                                actions
                                    .local_patch
                                    .insert(key.clone(), Value::from(val as f64));
                            }
                        });
                    });
                }
            }
            "number" => {
                if let Some(key) = &w.state_key {
                    let mut text = local_state
                        .get(key)
                        .and_then(|v| v.as_f64())
                        .map(|n| n.to_string())
                        .unwrap_or_else(|| "0".into());
                    let label = widget_text(w, doc, language).unwrap_or_else(|| key.clone());
                    ui.add_enabled_ui(enabled, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(label);
                            if ui.text_edit_singleline(&mut text).changed() {
                                if let Ok(n) = text.parse::<f64>() {
                                    actions.local_patch.insert(key.clone(), Value::from(n));
                                }
                            }
                        });
                    });
                }
            }
            "progress" => {
                let frac = if let Some(bind) = &w.bind {
                    resolve_bind(cache, bind, w.source.as_deref())
                } else if let Some(key) = &w.state_key {
                    local_state.get(key).cloned().unwrap_or(Value::from(0))
                } else {
                    Value::from(0)
                };
                let (done, total) = progress_values(&frac);
                let label = widget_text(w, doc, language).unwrap_or_else(|| "Progress".into());
                ui.label(label);
                ui.add(
                    egui::ProgressBar::new(done as f32 / total.max(1) as f32)
                        .text(format!("{done}/{total}")),
                );
            }
            "job" => {
                let t = crate::i18n::strings(language);
                let sub_id = w
                    .subscription
                    .as_deref()
                    .or(w.action.as_deref())
                    .unwrap_or("job");
                if let Some(job) = subscriptions.job(sub_id) {
                    let state_key = job.state.as_deref().unwrap_or("queued");
                    let state_label = crate::i18n::job_state_human_label(&t, state_key);
                    let heading = widget_text(w, doc, language)
                        .unwrap_or_else(|| t.decl_job_idle.to_string());
                    ui.label(heading);
                    ui.weak(state_label);
                    if let Some(p) = &job.progress {
                        ui.add(egui::ProgressBar::new(
                            p.completed as f32 / p.total.max(1) as f32,
                        )
                        .text(format!("{}/{}", p.completed, p.total)));
                    }
                    if matches!(state_key, "running" | "queued") {
                        if let Some(job_id) = &job.job_id {
                            if ui.button(t.decl_job_cancel).clicked() {
                                actions.cancel_job =
                                    Some((job_id.clone(), sub_id.to_string()));
                            }
                        }
                    }
                } else {
                    ui.weak(
                        widget_text(w, doc, language)
                            .unwrap_or_else(|| t.decl_job_idle.to_string()),
                    );
                }
            }
            "image_view" => {
                let path = image_view_path(w, cache, binding_cache, local_state);
                let id = w
                    .label_key
                    .clone()
                    .or_else(|| w.state_key.clone())
                    .unwrap_or_else(|| path.clone());
                let view = image_views.entry(id.clone()).or_default();
                ui.group(|ui| {
                    if let Some(tex) = try_load_png(ui.ctx(), &path) {
                        let size = tex.size_vec2() * view.zoom.max(0.1);
                        let offset = egui::vec2(view.pan[0], view.pan[1]);
                        ui.image((tex.id(), size));
                        let rect = ui.min_rect().translate(offset);
                        let response = ui.interact(rect, ui.id().with("iv"), egui::Sense::drag());
                        if response.dragged() {
                            view.pan[0] += response.drag_delta().x;
                            view.pan[1] += response.drag_delta().y;
                        }
                        if response.hovered() {
                            let scroll = ui.input(|i| i.raw_scroll_delta.y);
                            if scroll.abs() > 0.0 {
                                view.zoom = (view.zoom + scroll * 0.001).clamp(0.2, 8.0);
                            }
                        }
                    } else {
                        ui.weak(format!("image_view: {path}"));
                    }
                });
            }
            _ => {
                ui.colored_label(
                    egui::Color32::RED,
                    format!("unsupported widget: {}", w.kind),
                );
            }
        }
    }
}

pub fn ingest_tool_schemas(manifest_tools: &[ModuleTool], out: &mut HashMap<String, Value>) {
    for t in manifest_tools {
        out.insert(t.name.clone(), t.input_schema.clone());
    }
}

fn widget_text(w: &DeclUiWidget, doc: &DeclUiDocument, language: &str) -> Option<String> {
    widget_text_from_key(w.label_key.as_deref(), doc, language)
        .or_else(|| w.text.clone().filter(|t| !t.is_empty()))
}

fn widget_text_from_key(key: Option<&str>, doc: &DeclUiDocument, language: &str) -> Option<String> {
    let key = key.filter(|k| !k.is_empty())?;
    doc.labels.as_ref()?.resolve(language, key)
}

fn queue_invoke(
    actions: &mut DeclUiActions,
    tool: &str,
    args: Value,
    refresh_binds: Vec<String>,
    clear_form_keys: Vec<String>,
    tool_schemas: &HashMap<String, Value>,
) {
    if !tool_schemas.contains_key(tool) {
        return;
    }
    actions.invoke = Some(DeclUiInvokeAction {
        tool: tool.to_string(),
        args,
        refresh_binds,
        clear_form_keys,
    });
}

fn queue_service_action(
    actions: &mut DeclUiActions,
    action: &RichAction,
    local_state: &HashMap<String, Value>,
    document_state: &HashMap<String, Value>,
    doc: &DeclUiDocument,
) {
    let input = action
        .input
        .as_ref()
        .map(|t| resolve_action_input(t, local_state, document_state))
        .unwrap_or_else(|| Value::Object(Default::default()));
    let subscription_id = doc
        .subscriptions
        .iter()
        .find(|s| s.action.as_deref() == Some(action.id.as_str()))
        .map(|s| s.id.clone());
    actions.service_action = Some(DeclUiServiceAction {
        action_id: action.id.clone(),
        service: action.service.clone(),
        tool: action.tool.clone(),
        input,
        refresh_binds: action.refresh_binds.clone(),
        subscription_id,
    });
}

fn progress_values(val: &Value) -> (u32, u32) {
    if let Some(n) = val.as_u64() {
        return (n as u32, 100);
    }
    if let (Some(c), Some(t)) = (val.get("completed"), val.get("total")) {
        return (
            c.as_u64().unwrap_or(0) as u32,
            t.as_u64().unwrap_or(1) as u32,
        );
    }
    (0, 1)
}

fn image_view_path(
    w: &DeclUiWidget,
    cache: &HashMap<String, Value>,
    binding_cache: &HashMap<String, Value>,
    local_state: &HashMap<String, Value>,
) -> String {
    if let Some(resource) = &w.resource {
        if let Some(rest) = resource.strip_prefix("$local.") {
            if let Some(v) = local_state.get(rest).and_then(|x| x.as_str()) {
                return v.to_string();
            }
        }
        return resource.clone();
    }
    if let Some(binding_id) = &w.binding {
        if let Some(val) = binding_cache.get(binding_id) {
            if let Some(s) = val.as_str() {
                return s.to_string();
            }
            if let Some(s) = val.get("path").and_then(|p| p.as_str()) {
                return s.to_string();
            }
        }
    }
    media_path(w, cache)
}

fn render_form_fields(
    ui: &mut Ui,
    fields: &[SchemaField],
    form_fields: &mut HashMap<String, String>,
    inline: bool,
) {
    for field in fields {
        form_fields
            .entry(field.key.clone())
            .or_insert_with(|| field.default_string());
        if inline {
            match &field.kind {
                FieldKind::Bool => {
                    let mut on = form_fields
                        .get(&field.key)
                        .map(|s| s == "true")
                        .unwrap_or(false);
                    if ui.checkbox(&mut on, "").changed() {
                        form_fields.insert(
                            field.key.clone(),
                            if on { "true".into() } else { "false".into() },
                        );
                    }
                }
                FieldKind::Enum(vals) => {
                    let cur = form_fields.get(&field.key).cloned().unwrap_or_default();
                    egui::ComboBox::from_id_salt(format!("form-{}", field.key))
                        .selected_text(&cur)
                        .show_ui(ui, |ui| {
                            for v in vals {
                                ui.selectable_value(
                                    form_fields.get_mut(&field.key).unwrap(),
                                    v.clone(),
                                    v,
                                );
                            }
                        });
                }
                FieldKind::Textarea => {
                    ui.add(
                        egui::TextEdit::multiline(form_fields.get_mut(&field.key).unwrap())
                            .hint_text(field.hint.as_deref().unwrap_or("")),
                    );
                }
                FieldKind::Number | FieldKind::Text => {
                    ui.add(
                        egui::TextEdit::singleline(form_fields.get_mut(&field.key).unwrap())
                            .hint_text(field.hint.as_deref().unwrap_or("")),
                    );
                }
            }
        } else {
            ui.horizontal(|ui| {
                ui.label(&field.label);
                match &field.kind {
                    FieldKind::Bool => {
                        let mut on = form_fields
                            .get(&field.key)
                            .map(|s| s == "true")
                            .unwrap_or(false);
                        if ui.checkbox(&mut on, "").changed() {
                            form_fields.insert(
                                field.key.clone(),
                                if on { "true".into() } else { "false".into() },
                            );
                        }
                    }
                    FieldKind::Enum(vals) => {
                        let cur = form_fields.get(&field.key).cloned().unwrap_or_default();
                        egui::ComboBox::from_id_salt(format!("form-{}", field.key))
                            .selected_text(&cur)
                            .show_ui(ui, |ui| {
                                for v in vals {
                                    ui.selectable_value(
                                        form_fields.get_mut(&field.key).unwrap(),
                                        v.clone(),
                                        v,
                                    );
                                }
                            });
                    }
                    FieldKind::Textarea => {
                        ui.add(
                            egui::TextEdit::multiline(form_fields.get_mut(&field.key).unwrap())
                                .hint_text(field.hint.as_deref().unwrap_or("")),
                        );
                    }
                    FieldKind::Number | FieldKind::Text => {
                        ui.add(
                            egui::TextEdit::singleline(form_fields.get_mut(&field.key).unwrap())
                                .hint_text(field.hint.as_deref().unwrap_or("")),
                        );
                    }
                }
            });
        }
    }
}

fn submit_decl_form(
    actions: &mut DeclUiActions,
    tool: &str,
    fields: &[SchemaField],
    form_fields: &HashMap<String, String>,
    refresh_binds: Vec<String>,
    tool_schemas: &HashMap<String, Value>,
) {
    let mut args = serde_json::Map::new();
    let mut clear_keys = Vec::new();
    for field in fields {
        if let Some(v) = form_fields.get(&field.key) {
            args.insert(field.key.clone(), field.parse_value(v));
        }
        clear_keys.push(field.key.clone());
    }
    queue_invoke(
        actions,
        tool,
        Value::Object(args),
        refresh_binds,
        clear_keys,
        tool_schemas,
    );
}

fn bind_rows(val: &Value) -> Vec<Value> {
    match val {
        Value::Array(a) => a.clone(),
        Value::Object(o) => o
            .get("items")
            .or_else(|| o.get("rows"))
            .or_else(|| o.get("tasks"))
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn row_action_label(
    action: &DeclUiRowAction,
    doc: &DeclUiDocument,
    language: &str,
) -> Option<String> {
    if let Some(label) = action.label.as_deref().filter(|l| !l.is_empty()) {
        return Some(label.to_string());
    }
    widget_text_from_key(action.label_key.as_deref(), doc, language)
}

fn row_action_visible(action: &DeclUiRowAction, row: &Value) -> bool {
    let Some(when) = &action.when else {
        return true;
    };
    match row {
        Value::Object(map) => when.matches(map),
        _ => false,
    }
}

fn resolve_bind(cache: &HashMap<String, Value>, bind: &str, source: Option<&str>) -> Value {
    let base = cache.get(bind).cloned().unwrap_or(Value::Null);
    if let Some(src) = source.filter(|s| !s.is_empty()) {
        json_pointer_get(&base, src).unwrap_or(base)
    } else {
        base
    }
}

fn json_pointer_get(val: &Value, pointer: &str) -> Option<Value> {
    let path = pointer.trim_start_matches('/').trim_start_matches('.');
    if path.is_empty() {
        return Some(val.clone());
    }
    let mut cur = val;
    for part in path.split(['.', '/']) {
        if part.is_empty() {
            continue;
        }
        cur = cur.get(part)?;
    }
    Some(cur.clone())
}

fn schema_fields(schema: &Value, doc: &DeclUiDocument, language: &str) -> Vec<SchemaField> {
    let mut out = Vec::new();
    if let Some(props) = schema.get("properties").and_then(|p| p.as_object()) {
        for (k, v) in props {
            let label = doc
                .labels
                .as_ref()
                .and_then(|labels| labels.resolve(language, k))
                .or_else(|| {
                    v.get("title")
                        .or_else(|| v.get("description"))
                        .and_then(|x| x.as_str())
                        .map(str::to_string)
                })
                .unwrap_or_else(|| k.clone());
            let hint = v
                .get("x-hint-key")
                .and_then(|x| x.as_str())
                .and_then(|hint_key| doc.labels.as_ref()?.resolve(language, hint_key));
            out.push(SchemaField {
                key: k.clone(),
                label,
                hint,
                kind: FieldKind::from_schema(v),
            });
        }
    }
    out.sort_by(|a, b| a.key.cmp(&b.key));
    out
}

#[derive(Clone)]
struct SchemaField {
    key: String,
    label: String,
    hint: Option<String>,
    kind: FieldKind,
}

#[derive(Clone)]
enum FieldKind {
    Text,
    Textarea,
    Number,
    Bool,
    Enum(Vec<String>),
}

impl FieldKind {
    fn from_schema(v: &Value) -> Self {
        if let Some(arr) = v.get("enum").and_then(|e| e.as_array()) {
            let vals: Vec<String> = arr
                .iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect();
            if !vals.is_empty() {
                return FieldKind::Enum(vals);
            }
        }
        match v.get("type").and_then(|t| t.as_str()) {
            Some("boolean") => FieldKind::Bool,
            Some("integer") | Some("number") => FieldKind::Number,
            Some("string") => {
                let fmt = v.get("format").and_then(|f| f.as_str()).unwrap_or("");
                let long = v.get("maxLength").and_then(|m| m.as_u64()).unwrap_or(0) > 120;
                if fmt == "textarea" || long {
                    FieldKind::Textarea
                } else {
                    FieldKind::Text
                }
            }
            _ => FieldKind::Text,
        }
    }
}

impl SchemaField {
    fn default_string(&self) -> String {
        match &self.kind {
            FieldKind::Bool => "false".into(),
            FieldKind::Enum(v) => v.first().cloned().unwrap_or_default(),
            _ => String::new(),
        }
    }

    fn parse_value(&self, raw: &str) -> Value {
        match &self.kind {
            FieldKind::Bool => Value::Bool(raw == "true" || raw == "1"),
            FieldKind::Number => {
                if let Ok(i) = raw.parse::<i64>() {
                    Value::Number(i.into())
                } else if let Ok(f) = raw.parse::<f64>() {
                    serde_json::Number::from_f64(f)
                        .map(Value::Number)
                        .unwrap_or_else(|| Value::String(raw.to_string()))
                } else {
                    Value::String(raw.to_string())
                }
            }
            _ => Value::String(raw.to_string()),
        }
    }
}

fn render_stats(ui: &mut Ui, val: &Value, items: Option<&[String]>) {
    match val {
        Value::Object(map) => {
            let keys: Vec<_> = if let Some(items) = items {
                items.to_vec()
            } else {
                let mut k: Vec<_> = map.keys().cloned().collect();
                k.sort();
                k
            };
            for key in keys {
                if let Some(v) = map.get(&key) {
                    ui.label(format!("{key}: {}", value_display(v)));
                }
            }
        }
        Value::Array(arr) => {
            for item in arr {
                if let Value::Object(row) = item {
                    let label = row
                        .get("label")
                        .or_else(|| row.get("name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("—");
                    let value = row
                        .get("value")
                        .map(value_display)
                        .unwrap_or_else(|| "—".into());
                    ui.label(format!("{label}: {value}"));
                }
            }
        }
        other => {
            ui.label(value_display(other));
        }
    }
}

fn render_table(
    ui: &mut Ui,
    val: &Value,
    columns: Option<&[String]>,
    hide_headers: bool,
    row_actions: Option<&[DeclUiRowAction]>,
    doc: &DeclUiDocument,
    language: &str,
    tool_schemas: &HashMap<String, Value>,
    pending_invoke: bool,
    actions: &mut DeclUiActions,
) {
    let rows = bind_rows(val);
    if rows.is_empty() {
        return;
    }
    let show_actions = row_actions.is_some_and(|a| !a.is_empty());
    let cols: Vec<String> = if let Some(c) = columns {
        c.to_vec()
    } else if let Value::Object(first) = rows.first().unwrap_or(&Value::Null) {
        let mut k: Vec<_> = first.keys().cloned().collect();
        k.sort();
        k
    } else {
        vec!["value".into()]
    };
    egui::Grid::new("decl_ui_table")
        .striped(true)
        .show(ui, |ui| {
            if !hide_headers {
                for c in &cols {
                    ui.strong(c);
                }
                if show_actions {
                    ui.strong("");
                }
                ui.end_row();
            }
            for row in &rows {
                match row {
                    Value::Object(map) => {
                        for c in &cols {
                            ui.label(map.get(c).map(value_display).unwrap_or_else(|| "—".into()));
                        }
                        if let Some(actions_def) = row_actions {
                            ui.horizontal(|ui| {
                                for action in actions_def {
                                    if !row_action_visible(action, row) {
                                        continue;
                                    }
                                    let Some(label) = row_action_label(action, doc, language)
                                    else {
                                        continue;
                                    };
                                    let enabled = !pending_invoke && actions.invoke.is_none();
                                    if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                                        let args = resolve_row_args(&action.args, row);
                                        queue_invoke(
                                            actions,
                                            &action.tool,
                                            args,
                                            action.refresh_binds.clone().unwrap_or_default(),
                                            Vec::new(),
                                            tool_schemas,
                                        );
                                    }
                                }
                            });
                        }
                    }
                    Value::Array(cells) => {
                        for c in cols.iter().enumerate() {
                            let cell = cells
                                .get(c.0)
                                .map(value_display)
                                .unwrap_or_else(|| "—".into());
                            ui.label(cell);
                        }
                        if show_actions {
                            ui.label("");
                        }
                    }
                    other => {
                        ui.label(value_display(other));
                        for _ in 1..cols.len() {
                            ui.label("");
                        }
                        if show_actions {
                            ui.label("");
                        }
                    }
                }
                ui.end_row();
            }
        });
}

fn render_line_chart(ui: &mut Ui, val: &Value, series_key: Option<&str>) {
    let points = extract_series_points(val, series_key);
    if points.is_empty() {
        ui.weak("—");
        return;
    }
    Plot::new("decl_ui_plot").height(160.0).show(ui, |plot_ui| {
        plot_ui.line(
            Line::new(PlotPoints::from_iter(
                points.iter().enumerate().map(|(i, y)| [i as f64, *y]),
            ))
            .name(series_key.unwrap_or("series")),
        );
    });
}

fn render_bar_chart(ui: &mut Ui, val: &Value, series_key: Option<&str>) {
    let points = extract_series_points(val, series_key);
    if points.is_empty() {
        ui.weak("—");
        return;
    }
    Plot::new("decl_ui_bar").height(160.0).show(ui, |plot_ui| {
        let bars: Vec<Bar> = points
            .iter()
            .enumerate()
            .map(|(i, y)| Bar::new(i as f64, *y))
            .collect();
        plot_ui.bar_chart(BarChart::new(bars).name(series_key.unwrap_or("series")));
    });
}

fn extract_pie_slices(val: &Value) -> Vec<(String, f64)> {
    let mut out = Vec::new();
    let arr = match val {
        Value::Array(a) => a.as_slice(),
        Value::Object(o) => o
            .get("items")
            .or_else(|| o.get("slices"))
            .and_then(|v| v.as_array())
            .map(|a| a.as_slice())
            .unwrap_or(&[]),
        _ => &[],
    };
    for item in arr {
        match item {
            Value::Object(m) => {
                let label = m
                    .get("label")
                    .or_else(|| m.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("—")
                    .to_string();
                let value = m
                    .get("value")
                    .and_then(|v| v.as_f64())
                    .or_else(|| m.get("value").and_then(|v| v.as_i64()).map(|i| i as f64))
                    .unwrap_or(0.0);
                if value > 0.0 {
                    out.push((label, value));
                }
            }
            Value::Array(pair) if pair.len() >= 2 => {
                let label = pair[0].as_str().unwrap_or("—").to_string();
                let value = pair[1]
                    .as_f64()
                    .or_else(|| pair[1].as_i64().map(|i| i as f64))
                    .unwrap_or(0.0);
                if value > 0.0 {
                    out.push((label, value));
                }
            }
            _ => {}
        }
    }
    out
}

fn render_pie(ui: &mut Ui, val: &Value) {
    let slices = extract_pie_slices(val);
    if slices.is_empty() {
        ui.weak("—");
        return;
    }
    let total: f64 = slices.iter().map(|(_, v)| *v).sum();
    if total <= 0.0 {
        ui.weak("—");
        return;
    }
    let (rect, _resp) = ui.allocate_exact_size(egui::vec2(160.0, 160.0), egui::Sense::hover());
    let center = rect.center();
    let radius = rect.width().min(rect.height()) * 0.42;
    let palette = [
        egui::Color32::from_rgb(70, 130, 220),
        egui::Color32::from_rgb(220, 120, 70),
        egui::Color32::from_rgb(90, 180, 110),
        egui::Color32::from_rgb(180, 90, 180),
        egui::Color32::from_rgb(220, 180, 60),
        egui::Color32::from_rgb(90, 180, 200),
    ];
    let mut angle = -std::f32::consts::FRAC_PI_2;
    let painter = ui.painter();
    for (i, (label, value)) in slices.iter().enumerate() {
        let sweep = ((value / total) as f32) * std::f32::consts::TAU;
        let color = palette[i % palette.len()];
        let steps = ((sweep.abs() / 0.12).ceil() as usize).max(3);
        let mut points = vec![center];
        for s in 0..=steps {
            let a = angle + sweep * (s as f32 / steps as f32);
            points.push(center + egui::vec2(a.cos() * radius, a.sin() * radius));
        }
        painter.add(egui::Shape::convex_polygon(
            points,
            color,
            egui::Stroke::new(1.0_f32, egui::Color32::from_gray(30)),
        ));
        let mid = angle + sweep * 0.5;
        let tip = center + egui::vec2(mid.cos() * (radius * 0.62), mid.sin() * (radius * 0.62));
        if sweep > 0.25 {
            painter.text(
                tip,
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(11.0),
                egui::Color32::WHITE,
            );
        }
        angle += sweep;
    }
    ui.horizontal_wrapped(|ui| {
        for (i, (label, value)) in slices.iter().enumerate() {
            let color = palette[i % palette.len()];
            ui.colored_label(color, format!("{label}: {value:.1}"));
        }
    });
}

fn extract_xy_points(val: &Value, series_key: Option<&str>) -> Vec<[f64; 2]> {
    let mut out = Vec::new();
    let arr = match val {
        Value::Array(a) => a.as_slice(),
        Value::Object(o) => {
            if let Some(key) = series_key {
                if let Some(series) = o.get(key).and_then(|v| v.as_array()) {
                    series.as_slice()
                } else {
                    o.get("points")
                        .or_else(|| o.get("items"))
                        .and_then(|v| v.as_array())
                        .map(|a| a.as_slice())
                        .unwrap_or(&[])
                }
            } else {
                o.get("points")
                    .or_else(|| o.get("items"))
                    .and_then(|v| v.as_array())
                    .map(|a| a.as_slice())
                    .unwrap_or(&[])
            }
        }
        _ => &[],
    };
    for (i, item) in arr.iter().enumerate() {
        match item {
            Value::Object(m) => {
                let x = m
                    .get("x")
                    .and_then(|v| v.as_f64())
                    .or_else(|| m.get("x").and_then(|v| v.as_i64()).map(|n| n as f64))
                    .unwrap_or(i as f64);
                let y = m
                    .get("y")
                    .or_else(|| series_key.and_then(|k| m.get(k)))
                    .and_then(|v| v.as_f64())
                    .or_else(|| {
                        m.get("y")
                            .or_else(|| series_key.and_then(|k| m.get(k)))
                            .and_then(|v| v.as_i64())
                            .map(|n| n as f64)
                    })
                    .unwrap_or(0.0);
                out.push([x, y]);
            }
            Value::Array(pair) if pair.len() >= 2 => {
                let x = pair[0]
                    .as_f64()
                    .or_else(|| pair[0].as_i64().map(|n| n as f64))
                    .unwrap_or(i as f64);
                let y = pair[1]
                    .as_f64()
                    .or_else(|| pair[1].as_i64().map(|n| n as f64))
                    .unwrap_or(0.0);
                out.push([x, y]);
            }
            Value::Number(n) => {
                if let Some(y) = n.as_f64() {
                    out.push([i as f64, y]);
                }
            }
            _ => {}
        }
    }
    out
}

fn render_scatter(ui: &mut Ui, val: &Value, series_key: Option<&str>) {
    let points = extract_xy_points(val, series_key);
    if points.is_empty() {
        ui.weak("—");
        return;
    }
    Plot::new("decl_ui_scatter")
        .height(160.0)
        .show(ui, |plot_ui| {
            plot_ui.points(
                Points::new(PlotPoints::from_iter(points))
                    .radius(3.0_f32)
                    .name(series_key.unwrap_or("points")),
            );
        });
}

fn render_choice(
    ui: &mut Ui,
    w: &DeclUiWidget,
    form_fields: &mut HashMap<String, String>,
    radio: bool,
) {
    let key = w
        .label
        .clone()
        .or_else(|| w.text.clone())
        .unwrap_or_else(|| "choice".into());
    let items = w.items.clone().unwrap_or_default();
    form_fields
        .entry(key.clone())
        .or_insert_with(|| items.first().cloned().unwrap_or_default());
    if radio {
        ui.label(&key);
        for item in &items {
            ui.radio_value(form_fields.get_mut(&key).unwrap(), item.clone(), item);
        }
    } else {
        let cur = form_fields.get(&key).cloned().unwrap_or_default();
        egui::ComboBox::from_id_salt(format!("select-{key}"))
            .selected_text(&cur)
            .show_ui(ui, |ui| {
                for item in &items {
                    ui.selectable_value(form_fields.get_mut(&key).unwrap(), item.clone(), item);
                }
            });
    }
}

fn media_path(w: &DeclUiWidget, cache: &HashMap<String, Value>) -> String {
    if let Some(bind) = &w.bind {
        let val = resolve_bind(cache, bind, w.source.as_deref());
        if let Some(s) = val.as_str() {
            return s.to_string();
        }
        if let Some(s) = val.get("path").and_then(|p| p.as_str()) {
            return s.to_string();
        }
    }
    w.text.clone().unwrap_or_default()
}

pub(crate) fn host_file_from_logical(logical: &str) -> std::path::PathBuf {
    if let Ok(home) = std::env::var("AOS_HOME") {
        let rel = logical.trim_start_matches('/');
        return std::path::PathBuf::from(home)
            .join("var/storage/data")
            .join(rel);
    }
    std::path::PathBuf::from(logical)
}

pub(crate) fn try_load_png(ctx: &egui::Context, logical: &str) -> Option<egui::TextureHandle> {
    let path = host_file_from_logical(logical);
    let bytes = std::fs::read(&path).ok()?;
    if bytes.len() < 8 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let img = image::load_from_memory(&bytes).ok()?;
    let rgba = img.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
    Some(ctx.load_texture(logical, color, egui::TextureOptions::LINEAR))
}

pub(crate) fn open_host_path(logical: &str) -> std::io::Result<()> {
    let path = host_file_from_logical(logical);
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &path.to_string_lossy()])
            .spawn()?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open").arg(&path).spawn()?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(&path).spawn()?;
    }
    Ok(())
}

fn extract_series_points(val: &Value, series_key: Option<&str>) -> Vec<f64> {
    if let Value::Array(arr) = val {
        return arr
            .iter()
            .filter_map(|v| v.as_f64().or_else(|| v.as_i64().map(|i| i as f64)))
            .collect();
    }
    if let Some(key) = series_key {
        if let Some(v) = val.get(key) {
            return extract_series_points(v, None);
        }
    }
    if let Some(v) = val.get("series").or_else(|| val.get("points")) {
        return extract_series_points(v, None);
    }
    if let Value::Array(rows) = val.get("items").unwrap_or(&Value::Null) {
        return rows
            .iter()
            .filter_map(|r| {
                r.get("y")
                    .or_else(|| r.get("value"))
                    .and_then(|v| v.as_f64())
            })
            .collect();
    }
    Vec::new()
}

fn value_display(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "—".into(),
        other => serde_json::to_string(other).unwrap_or_else(|_| "—".into()),
    }
}
