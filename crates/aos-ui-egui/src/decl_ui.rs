//! Host-rendered declarative module UI (E15 / Preview 0.7).

use crate::icons;
use crate::rich_composition_ui::{patch_to_local_map, LayerCanvasHostState};
use crate::rich_decl::{
    init_state_from_schema, ImageViewInteractionState, JobProgressThrottle, RichDeclSubscriptions,
};
use crate::scene3d_ui::{patch_to_local_map as scene_patch_to_local_map, Scene3dHostState};
use aos_proto::decl_ui::{resolve_row_args, DeclUiDocument, DeclUiRowAction, DeclUiWidget};
use aos_proto::rich_decl_ui::{eval_predicate, resolve_action_input, RichAction, RichJobHandle};
use aos_proto::ModuleTool;
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
    pub layer_canvases: HashMap<String, LayerCanvasHostState>,
    pub scene3d_viewports: HashMap<String, Scene3dHostState>,
    pub job_throttle: JobProgressThrottle,
    pub form_fields: HashMap<String, String>,
    pub status: String,
    pub tool_schemas: HashMap<String, Value>,
    pub pending_invoke: bool,
    pub pending_refresh_binds: Vec<String>,
    pub pending_clear_form_keys: Vec<String>,
    /// Local state to apply after the next `set_document` (e.g. open-with-prompt
    /// seeds that would otherwise be wiped by schema defaults).
    pub pending_local_seed: HashMap<String, Value>,
    /// Last Create model/profile pair for which native generation defaults
    /// were applied. Kept outside declarative state so manual edits are not
    /// overwritten on every frame.
    pub create_preset_key: String,
}

impl DeclUiPanelState {
    pub fn new(module: impl Into<String>) -> Self {
        Self {
            module: module.into(),
            ..Default::default()
        }
    }

    /// True when the host still needs `ModuleUiLoad` (missing or failed document).
    /// When false, leave/re-enter must keep `local_state` and in-flight job handles.
    pub fn needs_ui_load(&self) -> bool {
        self.document.is_none()
    }

    pub fn set_document(&mut self, doc: DeclUiDocument) {
        self.error.clear();
        self.create_preset_key.clear();
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
        if !self.pending_local_seed.is_empty() {
            for (key, value) in self.pending_local_seed.drain() {
                self.local_state.insert(key, value);
            }
        }
    }

    pub fn seed_local(&mut self, key: impl Into<String>, value: Value) {
        let key = key.into();
        if self.document.is_some() {
            self.local_state.insert(key, value);
        } else {
            self.pending_local_seed.insert(key, value);
        }
    }

    pub fn close(&mut self) {
        self.subscriptions.clear();
        self.job_throttle.clear();
        self.image_views.clear();
        self.layer_canvases.clear();
        self.scene3d_viewports.clear();
        self.binding_cache.clear();
    }

    pub fn set_binding_result(&mut self, binding_id: &str, result: Value) {
        self.binding_cache.insert(binding_id.to_string(), result);
    }

    pub fn set_job_update(&mut self, subscription_id: &str, job: RichJobHandle) {
        if self
            .job_throttle
            .allow(job.job_id.as_deref().unwrap_or(subscription_id))
        {
            self.subscriptions.set_job(subscription_id, job);
        }
    }

    pub fn set_error(&mut self, msg: impl Into<String>) {
        self.document = None;
        self.error = msg.into();
    }

    pub fn set_bind_result(&mut self, tool: &str, result: Value) {
        if tool == "create.result.get" {
            if let Some(path) = result.get("path").and_then(Value::as_str) {
                self.local_state
                    .insert("result_path".into(), Value::String(path.into()));
            }
        }
        if matches!(
            tool,
            "illustration.project.create"
                | "illustration.project.open"
                | "illustration.project.import_legacy"
        ) {
            self.activate_illustration_project(&result);
        } else if tool == "illustration.project.load" {
            let expected = self.local_state.get("project_id").and_then(Value::as_str);
            if expected == result.get("project_id").and_then(Value::as_str) {
                if let Some(yaml) = result.get("yaml").and_then(Value::as_str) {
                    self.local_state.insert("scene".into(), Value::String(yaml.into()));
                }
            }
        } else if tool == "illustration.project.list" {
            self.local_state.insert(
                "legacy_available".into(),
                result
                    .get("legacy_available")
                    .cloned()
                    .unwrap_or(Value::Bool(false)),
            );
        } else if tool == "illustration.project.work_area" {
            if self.local_state.get("project_id") == result.get("project_id") {
                if let Some(area) = result.get("work_area") {
                    self.local_state.insert("work_area".into(), area.clone());
                    self.local_state.insert("last_work_area".into(), area.clone());
                }
            }
        } else if tool == "illustration.asset.register" {
            if self.local_state.get("project_id") == result.get("project_id") {
                let items = result.get("items").cloned().unwrap_or(Value::Array(Vec::new()));
                self.local_state.insert("project_assets".into(), items.clone());
                self.bind_cache.insert("illustration.asset.list".into(), serde_json::json!({ "items": items }));
            }
        } else if tool == "illustration.asset.select" {
            if self.local_state.get("project_id") == result.get("project_id") {
                let asset = &result["asset"];
                if asset.get("kind").and_then(Value::as_str) == Some("image") {
                    for (key, field) in [
                        ("library_image_uri", "uri"),
                        ("library_image_name", "name"),
                        ("library_image_prompt", "prompt"),
                    ] {
                        self.local_state.insert(
                            key.into(),
                            asset.get(field).cloned().unwrap_or(Value::String(String::new())),
                        );
                    }
                }
            }
        } else if tool == "illustration.asset.add" {
            if self.local_state.get("project_id") == result.get("project_id") {
                if let Some(yaml) = result.get("scene_yaml") {
                    self.local_state.insert("scene".into(), yaml.clone());
                }
            }
        } else if tool == "illustration.project.close" && result.get("closed") == Some(&Value::Bool(true)) {
            self.scene3d_viewports.clear();
            for key in ["project_id", "project_title", "scene", "beauty_path", "library_image_uri", "library_image_name", "library_image_prompt", "library_selected_asset_id", "library_selected_project_id", "library_error", "library_job_project_id", "library_job_id"] {
                self.local_state.insert(key.into(), Value::String(String::new()));
            }
            self.local_state.insert("library_section".into(), Value::String("assets".into()));
            self.local_state.insert("library_busy".into(), Value::Bool(false));
        }
        self.bind_cache.insert(tool.to_string(), result);
    }

    pub fn activate_illustration_project(&mut self, result: &Value) {
        let (Some(id), Some(yaml)) = (
            result.get("project_id").and_then(Value::as_str),
            result.get("yaml").and_then(Value::as_str),
        ) else {
            return;
        };
        self.scene3d_viewports.clear();
        for (key, value) in [
            ("project_id", Value::String(id.into())),
            ("project_title", result.get("title").cloned().unwrap_or(Value::Null)),
            ("scene", Value::String(yaml.into())),
            ("beauty_path", Value::String(String::new())),
            ("selected_id", Value::String(String::new())),
            ("compose_pending", Value::Bool(false)),
            ("project_assets", result.get("assets").cloned().unwrap_or(Value::Array(Vec::new()))),
            ("library_image_uri", Value::String(String::new())),
            ("library_image_name", Value::String(String::new())),
            ("library_image_prompt", Value::String(String::new())),
            ("library_selected_asset_id", Value::String(String::new())),
            ("library_selected_project_id", Value::String(String::new())),
            ("library_error", Value::String(String::new())),
            ("library_job_project_id", Value::String(String::new())),
            ("library_job_id", Value::String(String::new())),
            ("library_busy", Value::Bool(false)),
            ("library_section", Value::String("assets".into())),
            ("work_area", Value::String("start".into())),
            ("last_work_area", result.get("work_area").cloned().unwrap_or_else(|| Value::String("start".into()))),
        ] {
            self.local_state.insert(key.into(), value);
        }
        self.bind_cache.insert(
            "illustration.asset.list".into(),
            serde_json::json!({ "items": result.get("assets").cloned().unwrap_or(Value::Array(Vec::new())) }),
        );
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
            if self.module == "illustration-studio" {
                if let Some(version) = self
                    .local_state
                    .get("installed_version")
                    .and_then(Value::as_str)
                {
                    ui.weak(format!("v{version}"));
                }
                if let Some(project) = self
                    .local_state
                    .get("project_title")
                    .and_then(Value::as_str)
                    .filter(|title| !title.is_empty())
                {
                    ui.separator();
                    ui.strong(project);
                }
            }
            if ui.button(refresh_label).clicked() {
                actions.refresh = true;
            }
        });
        if !self.status.is_empty() {
            ui.weak(&self.status);
        }
        ui.separator();
        // Render the root node itself.  Unwrapping `root.children` here would
        // discard structural containers such as `split`, turning a two-pane
        // Create layout into a single vertical stream.
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
            &mut self.layer_canvases,
            &mut self.scene3d_viewports,
            &mut self.form_fields,
            &self.tool_schemas,
            self.pending_invoke,
            &mut actions,
        );
        if self.module == "illustration-studio" && actions.service_action.is_none() {
            let shortcut = ui.input(|input| {
                if input.modifiers.command && input.key_pressed(egui::Key::S) {
                    Some("save_project")
                } else if input.modifiers.command
                    && input.modifiers.shift
                    && input.key_pressed(egui::Key::R)
                {
                    Some("blender_beauty")
                } else {
                    None
                }
            });
            if let Some(id) = shortcut {
                if let Some(action) = doc.actions.iter().find(|action| action.id == id) {
                    queue_service_action(
                        &mut actions,
                        action,
                        &self.local_state,
                        &self.document_state,
                        &doc,
                    );
                }
            }
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
        layer_canvases: &mut HashMap<String, LayerCanvasHostState>,
        scene3d_viewports: &mut HashMap<String, Scene3dHostState>,
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
            "plain_column" => {
                if let Some(children) = &w.children {
                    for child in children {
                        Self::render_widget(
                            ui,
                            md_cache,
                            child,
                            doc,
                            language,
                            cache,
                            binding_cache,
                            local_state,
                            document_state,
                            subscriptions,
                            image_views,
                            layer_canvases,
                            scene3d_viewports,
                            form_fields,
                            tool_schemas,
                            pending_invoke,
                            actions,
                        );
                    }
                }
            }
            "column" => {
                let h = ui.available_height();
                let column_salt = (
                    w.label_key.as_deref(),
                    w.label.as_deref(),
                    w.children.as_ref().map(|c| c.len()).unwrap_or(0),
                    // Distinguish left/right split panes that share kind+len.
                    ui.id(),
                );
                egui::ScrollArea::vertical()
                    .id_salt(("decl_ui_column", column_salt))
                    .max_height(h.max(120.0))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = crate::theme::SPACE_UNIT;
                        ui.vertical(|ui| {
                            if let Some(children) = &w.children {
                                for (idx, c) in children.iter().enumerate() {
                                    if idx > 0 && c.kind == "section" {
                                        // Extra rhythm above section headers so
                                        // boxed groups and disclosures separate.
                                        ui.add_space(crate::theme::SPACE_UNIT);
                                    }
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
                                        layer_canvases,
                                        scene3d_viewports,
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
            "section" => {
                let title = widget_text(w, doc, language);
                let mut render_children = |ui: &mut Ui| {
                    ui.spacing_mut().item_spacing.y = crate::theme::SPACE_UNIT;
                    if let Some(children) = &w.children {
                        for child in children {
                            Self::render_widget(
                                ui,
                                md_cache,
                                child,
                                doc,
                                language,
                                cache,
                                binding_cache,
                                local_state,
                                document_state,
                                subscriptions,
                                image_views,
                                layer_canvases,
                                scene3d_viewports,
                                form_fields,
                                tool_schemas,
                                pending_invoke,
                                actions,
                            );
                        }
                    }
                };
                if w.collapsible.unwrap_or(false) {
                    let open_key = w
                        .open_state_key
                        .clone()
                        .unwrap_or_else(|| "advanced_open".into());
                    let default_open = local_state
                        .get(&open_key)
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    let header = title.unwrap_or_else(|| "…".into());
                    egui::CollapsingHeader::new(header)
                        .id_salt(format!("decl-section-{}", open_key))
                        .default_open(default_open)
                        .show(ui, |ui| render_children(ui));
                } else {
                    ui.group(|ui| {
                        if let Some(title) = title {
                            ui.heading(title);
                            ui.add_space(4.0);
                        }
                        render_children(ui);
                    });
                }
            }
            "row" => {
                let toolbar = w.toolbar.unwrap_or(false);
                if toolbar {
                    ui.add_space(2.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
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
                                    layer_canvases,
                                    scene3d_viewports,
                                    form_fields,
                                    tool_schemas,
                                    pending_invoke,
                                    actions,
                                );
                            }
                        }
                    });
                } else {
                    // Wrap so long FR labels + fields never force siblings into
                    // the neighboring split pane.
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = crate::theme::SPACE_UNIT;
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
                                    layer_canvases,
                                    scene3d_viewports,
                                    form_fields,
                                    tool_schemas,
                                    pending_invoke,
                                    actions,
                                );
                            }
                        }
                    });
                }
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
                        w.column_label_keys.as_deref(),
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
                render_choice(
                    ui,
                    w,
                    doc,
                    language,
                    binding_cache,
                    local_state,
                    form_fields,
                    actions,
                    enabled,
                    false,
                );
            }
            "radio" => {
                render_choice(
                    ui,
                    w,
                    doc,
                    language,
                    binding_cache,
                    local_state,
                    form_fields,
                    actions,
                    enabled,
                    true,
                );
            }
            "multiselect" => {
                let Some(state_key) = &w.state_key else {
                    return;
                };
                let label = widget_text(w, doc, language).unwrap_or_else(|| state_key.clone());
                let dynamic_items = w.binding.as_ref().and_then(|binding_id| {
                    let mut value = binding_cache
                        .get(binding_id)
                        .cloned()
                        .unwrap_or(Value::Null);
                    if let Some(source) = w.source.as_deref() {
                        if let Some(slice) = value.pointer(source) {
                            value = slice.clone();
                        }
                    }
                    value.as_array().map(|rows| {
                        rows.iter()
                            .filter_map(|row| {
                                row.as_str()
                                    .map(|s| (s.to_string(), s.to_string()))
                                    .or_else(|| {
                                        row.get("id").and_then(Value::as_str).map(|id| {
                                            let text = row
                                                .get("label")
                                                .and_then(Value::as_str)
                                                .unwrap_or(id);
                                            (id.to_string(), text.to_string())
                                        })
                                    })
                            })
                            .collect::<Vec<_>>()
                    })
                });
                let items: Vec<(String, String)> = dynamic_items.unwrap_or_else(|| {
                    w.items
                        .clone()
                        .unwrap_or_default()
                        .into_iter()
                        .enumerate()
                        .map(|(index, item)| {
                            let text = w
                                .item_label_keys
                                .as_ref()
                                .and_then(|keys| keys.get(index))
                                .and_then(|key| widget_text_from_key(Some(key), doc, language))
                                .unwrap_or_else(|| item.clone());
                            (item, text)
                        })
                        .collect()
                });
                let mut selected: Vec<String> = local_state
                    .get(state_key)
                    .and_then(Value::as_array)
                    .map(|values| {
                        values
                            .iter()
                            .filter_map(Value::as_str)
                            .map(ToOwned::to_owned)
                            .collect()
                    })
                    .unwrap_or_default();
                ui.label(label);
                for (item, item_label) in &items {
                    let mut checked = selected.iter().any(|value| value == item);
                    if ui
                        .add_enabled(enabled, egui::Checkbox::new(&mut checked, item_label))
                        .changed()
                    {
                        if checked {
                            if !selected.iter().any(|value| value == item) {
                                selected.push(item.to_string());
                            }
                        } else {
                            selected.retain(|value| value != item);
                        }
                        actions.local_patch.insert(
                            state_key.clone(),
                            Value::Array(selected.iter().cloned().map(Value::String).collect()),
                        );
                    }
                }
            }
            "prompt_starters" => {
                let t = crate::i18n::strings(language);
                let Some(state_key) = &w.state_key else {
                    return;
                };
                let label = widget_text(w, doc, language).unwrap_or_else(|| "Suggestions".into());
                let items: Vec<(String, String)> = w
                    .items
                    .clone()
                    .unwrap_or_default()
                    .into_iter()
                    .enumerate()
                    .map(|(index, item)| {
                        let text = w
                            .item_label_keys
                            .as_ref()
                            .and_then(|keys| keys.get(index))
                            .and_then(|key| widget_text_from_key(Some(key), doc, language))
                            .unwrap_or_else(|| item.clone());
                        (item, text)
                    })
                    .collect();
                let current = local_state
                    .get(state_key)
                    .and_then(Value::as_str)
                    .filter(|value| items.iter().any(|(_, label)| label == value))
                    .unwrap_or(t.decl_starter_placeholder)
                    .to_string();
                ui.horizontal(|ui| {
                    ui.label(label);
                    ui.add_enabled_ui(enabled, |ui| {
                        egui::ComboBox::from_id_salt(format!("prompt-starter-{state_key}"))
                            .selected_text(current)
                            .show_ui(ui, |ui| {
                                for (_item, item_label) in &items {
                                    if ui.selectable_label(false, item_label).clicked() {
                                        actions.local_patch.insert(
                                            state_key.clone(),
                                            Value::String(item_label.clone()),
                                        );
                                        ui.close_menu();
                                    }
                                }
                            });
                    });
                });
            }
            "asset_import" => {
                let t = crate::i18n::strings(language);
                let kind_key = w.state_key.as_deref().unwrap_or("asset_import_kind");
                let path_key = w
                    .source
                    .as_deref()
                    .unwrap_or("asset_import_path")
                    .trim_start_matches("$local.");
                let status_key = w
                    .binding
                    .as_deref()
                    .unwrap_or("asset_import_status")
                    .trim_start_matches("$local.");
                let mut kind = local_state
                    .get(kind_key)
                    .and_then(Value::as_str)
                    .unwrap_or("lora")
                    .to_string();
                let current_path = local_state
                    .get(path_key)
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let mut status = local_state
                    .get(status_key)
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let title = widget_text(w, doc, language)
                    .unwrap_or_else(|| t.decl_asset_import_title.to_string());
                ui.group(|ui| {
                    ui.label(title);
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt(format!("asset-kind-{kind_key}"))
                            .selected_text(kind.as_str())
                            .show_ui(ui, |ui| {
                                for candidate in ["lora", "vae", "style"] {
                                    if ui.selectable_value(&mut kind, candidate.to_string(), candidate).changed() {
                                        actions.local_patch.insert(kind_key.to_string(), Value::String(kind.clone()));
                                    }
                                }
                            });
                        if ui.add_enabled(enabled, egui::Button::new(t.decl_asset_import_choose)).clicked() {
                            let filters: &[(&str, &[&str])] = match kind.as_str() {
                                "style" => &[("Styles", &["txt"][..]), ("All files", &["*"][..])],
                                _ => &[("Weights", &["safetensors", "ckpt", "pt", "bin"][..]), ("All files", &["*"][..])],
                            };
                            if let Some(path) = crate::os_open::pick_os_file(
                                t.decl_asset_import_title,
                                filters,
                                crate::os_open::user_downloads_dir().as_deref(),
                            ) {
                                if let Some(path) = path.to_str() {
                                    actions.local_patch.insert(path_key.to_string(), Value::String(path.to_string()));
                                }
                            }
                        }
                        let shown = std::path::Path::new(&current_path)
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or(if current_path.is_empty() {
                                t.decl_asset_import_none
                            } else {
                                &current_path
                            });
                        ui.weak(shown);
                        if ui.add_enabled(enabled && !current_path.trim().is_empty(), egui::Button::new(t.decl_asset_import_submit)).clicked() {
                            match import_decl_asset(std::path::Path::new(&current_path), &kind) {
                                Ok(name) => {
                                    status = format!("Asset importé : {name}");
                                    actions.local_patch.insert(status_key.to_string(), Value::String(status.clone()));
                                    actions.local_patch.insert(path_key.to_string(), Value::String(String::new()));
                                    if kind == "style" {
                                        let mut selected = local_state.get("styles").and_then(Value::as_array).cloned().unwrap_or_default();
                                        if !selected.iter().any(|v| v.as_str() == Some(&name)) { selected.push(Value::String(name)); }
                                        actions.local_patch.insert("styles".into(), Value::Array(selected));
                                    } else if kind == "lora" {
                                        let mut selected = local_state.get("loras").and_then(Value::as_array).cloned().unwrap_or_default();
                                        if !selected.iter().any(|v| v.as_str() == Some(&name)) { selected.push(Value::String(name)); }
                                        actions.local_patch.insert("loras".into(), Value::Array(selected));
                                    } else {
                                        actions.local_patch.insert("vae".into(), Value::String(name));
                                    }
                                }
                                Err(error) => {
                                    status = format!("Échec de l’import : {error}");
                                    actions.local_patch.insert(status_key.to_string(), Value::String(status.clone()));
                                }
                            }
                        }
                    });
                    if kind == "style" {
                        let mut custom = local_state
                            .get("asset_custom_style")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string();
                        ui.horizontal(|ui| {
                            ui.label("Style personnalisé");
                            if ui
                                .add_enabled(enabled, egui::TextEdit::singleline(&mut custom).hint_text("fragment de prompt"))
                                .changed()
                            {
                                actions.local_patch.insert("asset_custom_style".into(), Value::String(custom.clone()));
                            }
                            if ui
                                .add_enabled(enabled && !custom.trim().is_empty(), egui::Button::new("Ajouter le style"))
                                .clicked()
                            {
                                match add_decl_custom_style(&custom) {
                                    Ok(style) => {
                                        let mut selected = local_state.get("styles").and_then(Value::as_array).cloned().unwrap_or_default();
                                        if !selected.iter().any(|v| v.as_str() == Some(&style)) { selected.push(Value::String(style.clone())); }
                                        actions.local_patch.insert("styles".into(), Value::Array(selected));
                                        actions.local_patch.insert("asset_custom_style".into(), Value::String(String::new()));
                                        actions.local_patch.insert(status_key.to_string(), Value::String(format!("Style ajouté : {style}")));
                                    }
                                    Err(error) => {
                                        actions.local_patch.insert(status_key.to_string(), Value::String(format!("Échec : {error}")));
                                    }
                                };
                            }
                        });
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Civitai (LoRA / styles)").clicked() {
                            crate::os_open::open_url("https://civitai.com/models?types=LORA&sort=Most+Downloaded");
                        }
                        if ui.button("Hugging Face (modèles)").clicked() {
                            crate::os_open::open_url("https://huggingface.co/models?pipeline_tag=text-to-image&sort=downloads");
                        }
                    });
                    if !status.is_empty() { ui.weak(status); }
                });
            }
            "checkbox" => {
                if let Some(state_key) = &w.state_key {
                    let label = widget_text(w, doc, language).unwrap_or_else(|| state_key.clone());
                    let edited_active = local_state
                        .get("use_edited_enriched")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    let checkbox_enabled = enabled
                        && !(edited_active
                            && matches!(
                                state_key.as_str(),
                                "enrich_prompt" | "enhance_prompt_chat"
                            ));
                    let mut on = local_state
                        .get(state_key)
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    if ui
                        .add_enabled(checkbox_enabled, egui::Checkbox::new(&mut on, label))
                        .changed()
                    {
                        actions
                            .local_patch
                            .insert(state_key.clone(), Value::Bool(on));
                        // An edited enriched prompt is an explicit source of
                        // truth: selecting it disables both automatic
                        // assistants. Conversely, opting into an assistant
                        // leaves edited-prompt mode so it cannot be silently
                        // overwritten by a later generation pass.
                        if state_key == "use_edited_enriched" && on {
                            actions
                                .local_patch
                                .insert("enrich_prompt".into(), Value::Bool(false));
                            actions
                                .local_patch
                                .insert("enhance_prompt_chat".into(), Value::Bool(false));
                        } else if matches!(
                            state_key.as_str(),
                            "enrich_prompt" | "enhance_prompt_chat"
                        ) && on
                        {
                            actions
                                .local_patch
                                .insert("use_edited_enriched".into(), Value::Bool(false));
                        }
                    }
                    return;
                }
                let key = {
                    w.label
                        .clone()
                        .or_else(|| w.text.clone())
                        .unwrap_or_else(|| "flag".into())
                };
                let mut on = form_fields.get(&key).map(|s| s == "true").unwrap_or(false);
                if ui.checkbox(&mut on, &key).changed() {
                    form_fields.insert(key, if on { "true".into() } else { "false".into() });
                }
            }
            "textarea" => {
                if let Some(state_key) = &w.state_key {
                    let label = widget_text(w, doc, language).unwrap_or_else(|| state_key.clone());
                    let mut text = local_state
                        .get(state_key)
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    ui.add_enabled_ui(enabled, |ui| {
                        ui.label(&label);
                        if ui
                            .add_sized(
                                [ui.available_width(), 92.0],
                                egui::TextEdit::multiline(&mut text)
                                    .desired_rows(4)
                                    .hint_text("Décrivez ce que vous voulez créer…"),
                            )
                            .changed()
                        {
                            actions
                                .local_patch
                                .insert(state_key.clone(), Value::String(text));
                        }
                    });
                } else {
                    let key = w.label.clone().unwrap_or_else(|| "text".into());
                    form_fields.entry(key.clone()).or_default();
                    ui.label(&key);
                    ui.add_sized(
                        [ui.available_width(), 92.0],
                        egui::TextEdit::multiline(form_fields.get_mut(&key).unwrap())
                            .desired_rows(4),
                    );
                }
            }
            "text_input" => {
                if let Some(state_key) = &w.state_key {
                    let label = widget_text(w, doc, language).unwrap_or_else(|| state_key.clone());
                    let mut text = local_state
                        .get(state_key)
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    ui.add_enabled_ui(enabled, |ui| {
                        // Label above field — matches Settings form rhythm and
                        // keeps FR labels from crushing the edit height in a row.
                        ui.vertical(|ui| {
                            ui.label(label);
                            let field_w = ui.available_width().clamp(120.0, 480.0);
                            let read_only = w.read_only.unwrap_or(false);
                            if crate::theme::add_form_field(
                                ui,
                                field_w,
                                egui::TextEdit::singleline(&mut text).interactive(!read_only),
                            )
                            .changed()
                                && !read_only
                            {
                                actions
                                    .local_patch
                                    .insert(state_key.clone(), Value::String(text));
                            }
                        });
                    });
                }
            }
            "file_picker" => {
                if let Some(state_key) = &w.state_key {
                    let t = crate::i18n::strings(language);
                    let label = widget_text(w, doc, language).unwrap_or_else(|| state_key.clone());
                    let current = local_state
                        .get(state_key)
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    ui.horizontal(|ui| {
                        ui.label(&label);
                        let shown = if current.is_empty() {
                            t.decl_file_picker_none.to_string()
                        } else {
                            std::path::Path::new(&current)
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or(&current)
                                .to_string()
                        };
                        ui.weak(shown);
                        if ui
                            .add_enabled(enabled, egui::Button::new(t.decl_file_picker_choose))
                            .clicked()
                        {
                            if let Some(path) = crate::os_open::pick_os_file(
                                &label,
                                &[("Images", &["png", "jpg", "jpeg", "webp"])],
                                crate::os_open::user_downloads_dir().as_deref(),
                            ) {
                                if let Some(logical) = import_decl_media_file(&path) {
                                    actions
                                        .local_patch
                                        .insert(state_key.clone(), Value::String(logical));
                                }
                            }
                        }
                    });
                }
            }
            "image" => {
                let path = media_path(w, cache);
                let catalogue_thumb = path.starts_with("/assets/illustration/catalogue/thumbs/");
                if !catalogue_thumb {
                    ui.label(format!("image: {path}"));
                }
                if let Some(tex) = try_load_png(ui.ctx(), &path) {
                    if catalogue_thumb {
                        ui.add(egui::Image::new(&tex).fit_to_exact_size(egui::vec2(96.0, 96.0)));
                    } else {
                        ui.image(&tex);
                    }
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
                let downloading = local_state
                    .get("dependency_download_active")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let install_action = matches!(
                    w.action.as_deref(),
                    Some("install_blender" | "install_trellis")
                );
                let can_run = enabled
                    && !pending_invoke
                    && actions.invoke.is_none()
                    && !(install_action && downloading);
                let tooltip = widget_tooltip(w, doc, language).unwrap_or_else(|| label.clone());
                let icon = w
                    .icon_key
                    .as_deref()
                    .and_then(icons::resolve_decl_action_icon)
                    .or_else(|| {
                        w.action
                            .as_deref()
                            .and_then(icons::resolve_decl_action_icon_for_action)
                    });
                let clicked = if let Some(icon) = icon {
                    icons::decl_action_button(
                        ui,
                        icon,
                        &label,
                        &tooltip,
                        can_run,
                        w.primary.unwrap_or(false),
                    )
                } else {
                    let response = ui.add_enabled(can_run, egui::Button::new(&label));
                    let response = response.on_hover_text(tooltip);
                    response.clicked()
                };
                if clicked {
                    if w.action.as_deref() == Some("import_glb") {
                        if let Some(path) = crate::os_open::pick_os_file(
                            &label,
                            &[("GLB 3D", &["glb"])],
                            crate::os_open::user_downloads_dir().as_deref(),
                        ) {
                            if let Some(action) = doc.actions.iter().find(|a| a.id == "import_glb")
                            {
                                queue_service_action(
                                    actions,
                                    action,
                                    local_state,
                                    document_state,
                                    doc,
                                );
                                if let Some(Value::Object(input)) =
                                    actions.service_action.as_mut().map(|a| &mut a.input)
                                {
                                    input.insert(
                                        "path".into(),
                                        Value::String(path.to_string_lossy().into_owned()),
                                    );
                                }
                            }
                        }
                    } else if w.action.as_deref() == Some("clear_preview") {
                        actions
                            .local_patch
                            .insert("result_path".into(), Value::String(String::new()));
                        actions
                            .local_patch
                            .insert("preview_cleared".into(), Value::Bool(true));
                    } else if w.action.as_deref() == Some("clear_layers") {
                        actions
                            .local_patch
                            .insert("composition_layers".into(), Value::Array(Vec::new()));
                        actions
                            .local_patch
                            .insert("composition_selected".into(), Value::Null);
                    } else if let Some(action_id) = &w.action {
                        if let Some(action) = doc.actions.iter().find(|a| &a.id == action_id) {
                            queue_service_action(actions, action, local_state, document_state, doc);
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
                    .id_salt(format!(
                        "decl_scroll_{}",
                        w.label_key.as_deref().unwrap_or("")
                    ))
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
                                    layer_canvases,
                                    scene3d_viewports,
                                    form_fields,
                                    tool_schemas,
                                    pending_invoke,
                                    actions,
                                );
                            }
                        }
                    });
            }
            "illustration_work_split" => {
                if let Some(children) = w.children.as_ref().filter(|children| children.len() == 2) {
                    let ratio = w.split_ratio.unwrap_or(0.34).clamp(0.1, 0.9);
                    let available = ui.available_size();
                    let (frame, _) = ui.allocate_exact_size(available, egui::Sense::hover());
                    let gap = crate::theme::SPACE_UNIT;
                    let left_width = ((frame.width() - gap) * ratio).max(1.0);
                    let left = egui::Rect::from_min_size(frame.min, egui::vec2(left_width, frame.height()));
                    let right = egui::Rect::from_min_max(
                        egui::pos2(left.right() + gap, frame.top()), frame.max,
                    );
                    for (child, rect) in children.iter().zip([left, right]) {
                        ui.scope_builder(
                            egui::UiBuilder::new()
                                .max_rect(rect)
                                .layout(egui::Layout::top_down(egui::Align::LEFT)),
                            |ui| {
                                ui.set_clip_rect(rect);
                                Self::render_widget(
                                    ui, md_cache, child, doc, language, cache, binding_cache,
                                    local_state, document_state, subscriptions, image_views,
                                    layer_canvases, scene3d_viewports, form_fields, tool_schemas,
                                    pending_invoke, actions,
                                );
                            },
                        );
                    }
                }
            }
            "split" => {
                let ratio = w.split_ratio.unwrap_or(0.5).clamp(0.1, 0.9);
                if let Some(children) = &w.children {
                    if children.len() == 2 {
                        // Give the split an explicit frame so both panes receive
                        // the complete height of the host panel, even when their
                        // initial content is short or an image is not loaded yet.
                        // Clip each pane so a wide control (e.g. preset row) cannot
                        // paint over the sibling pane (Create upscale vs save-preset).
                        let available = ui.available_size();
                        let gap = crate::theme::SPACE_UNIT;
                        ui.allocate_ui_with_layout(
                            available,
                            egui::Layout::left_to_right(egui::Align::TOP),
                            |ui| {
                                let pane_height = ui.available_height();
                                let total_w = ui.available_width();
                                let w_left = ((total_w - gap) * ratio).max(1.0);
                                let w_right = (total_w - gap - w_left).max(1.0);
                                ui.allocate_ui_with_layout(
                                    egui::vec2(w_left, pane_height),
                                    egui::Layout::top_down(egui::Align::LEFT),
                                    |ui| {
                                        ui.set_clip_rect(ui.max_rect());
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
                                            layer_canvases,
                                            scene3d_viewports,
                                            form_fields,
                                            tool_schemas,
                                            pending_invoke,
                                            actions,
                                        );
                                    },
                                );
                                ui.add_space(gap);
                                ui.allocate_ui_with_layout(
                                    egui::vec2(w_right, pane_height),
                                    egui::Layout::top_down(egui::Align::LEFT),
                                    |ui| {
                                        ui.set_clip_rect(ui.max_rect());
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
                                            layer_canvases,
                                            scene3d_viewports,
                                            form_fields,
                                            tool_schemas,
                                            pending_invoke,
                                            actions,
                                        );
                                    },
                                );
                            },
                        );
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
                    let tab_id = ui
                        .id()
                        .with(("decl-tabs", w.label_key.as_deref(), labels.len()));
                    let controlled = w
                        .state_key
                        .as_ref()
                        .zip(w.items.as_ref())
                        .filter(|(_, items)| items.len() == labels.len());
                    let mut selected = if let Some((key, items)) = controlled {
                        let current = local_state.get(key).and_then(Value::as_str);
                        items.iter().position(|item| Some(item.as_str()) == current).unwrap_or(0)
                    } else {
                        ui.memory(|memory| memory.data.get_temp::<usize>(tab_id))
                            .unwrap_or(0)
                            .min(labels.len().saturating_sub(1))
                    };
                    ui.horizontal(|ui| {
                        for (i, label) in labels.iter().enumerate() {
                            if ui.add_enabled(
                                !pending_invoke,
                                egui::SelectableLabel::new(selected == i, label),
                            ).clicked() {
                                selected = i;
                                if let Some((key, items)) = controlled {
                                    actions.local_patch.insert(key.clone(), Value::String(items[i].clone()));
                                    if key == "work_area" {
                                        if let Some(project_id) = local_state.get("project_id").and_then(Value::as_str) {
                                            queue_invoke(
                                                actions,
                                                "illustration.project.work_area",
                                                serde_json::json!({ "project_id": project_id, "work_area": items[i] }),
                                                Vec::new(),
                                                Vec::new(),
                                                tool_schemas,
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    });
                    if controlled.is_none() {
                        ui.memory_mut(|memory| memory.data.insert_temp(tab_id, selected));
                    }
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
                                layer_canvases,
                                scene3d_viewports,
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
                        ui.add(
                            egui::ProgressBar::new(p.completed as f32 / p.total.max(1) as f32)
                                .text(format!("{}/{}", p.completed, p.total)),
                        );
                    }
                    if let Some(err) = job.error.as_deref().filter(|s| !s.trim().is_empty() && sub_id != "library_image_job") {
                        ui.colored_label(egui::Color32::from_rgb(220, 80, 80), err);
                    }
                    if matches!(state_key, "running" | "queued") {
                        if let Some(job_id) = &job.job_id {
                            if ui.button(t.decl_job_cancel).clicked() {
                                actions.cancel_job = Some((job_id.clone(), sub_id.to_string()));
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
                let t = crate::i18n::strings(language);
                let path = image_view_path(w, cache, binding_cache, local_state);
                let id = w
                    .label_key
                    .clone()
                    .or_else(|| w.state_key.clone())
                    .unwrap_or_else(|| "preview".into());
                let view = image_views.entry(id.clone()).or_default();
                // Fill remaining stage height so Beauty output is a real pane,
                // not a thin strip under a fixed viewport (Illustration Studio).
                let avail_h = ui.available_height();
                let panel_h = if avail_h.is_finite() && avail_h > 120.0 {
                    avail_h.clamp(240.0, 900.0)
                } else {
                    420.0
                };
                let panel_size = egui::vec2(ui.available_width().max(280.0), panel_h);
                ui.allocate_ui_with_layout(
                    panel_size,
                    egui::Layout::top_down(egui::Align::Center),
                    |ui| {
                        ui.group(|ui| {
                            ui.set_min_size(panel_size - egui::vec2(8.0, 8.0));
                            if let Some(label) =
                                widget_text_from_key(w.label_key.as_deref(), doc, language)
                            {
                                ui.label(egui::RichText::new(label).strong());
                            }
                            if let Some(tex) = try_load_png(ui.ctx(), &path) {
                                let max_w = ui.available_width().max(1.0);
                                let max_h = ui.available_height().max(1.0);
                                let base = tex.size_vec2();
                                // Fill the assigned pane (Beauty stage); do not
                                // cap at 1.0 or small NPR/CPU PNGs look like a stamp.
                                let fit = (max_w / base.x.max(1.0)).min(max_h / base.y.max(1.0));
                                // Shared clamp for display and scroll so zoom state matches pixels.
                                const ZOOM_MIN: f32 = 0.2;
                                const ZOOM_MAX: f32 = 8.0;
                                // Treat unset/zero zoom as 1.0 so previews fill the panel.
                                let zoom = if view.zoom <= 0.0 {
                                    1.0
                                } else {
                                    view.zoom.clamp(ZOOM_MIN, ZOOM_MAX)
                                };
                                let size = base * fit * zoom;
                                let offset = egui::vec2(view.pan[0], view.pan[1]);
                                ui.image((tex.id(), size));
                                let rect = ui.min_rect().translate(offset);
                                let response =
                                    ui.interact(rect, ui.id().with("iv"), egui::Sense::drag());
                                if response.dragged() {
                                    view.pan[0] += response.drag_delta().x;
                                    view.pan[1] += response.drag_delta().y;
                                }
                                if response.hovered() {
                                    let scroll = ui.input(|i| i.raw_scroll_delta.y);
                                    if scroll.abs() > 0.0 {
                                        let base_zoom =
                                            if view.zoom <= 0.0 { 1.0 } else { view.zoom };
                                        view.zoom =
                                            (base_zoom + scroll * 0.001).clamp(ZOOM_MIN, ZOOM_MAX);
                                    }
                                }
                            } else {
                                ui.vertical_centered(|ui| {
                                    ui.add_space(48.0);
                                    // Title already rendered above when label_key is set.
                                    if w.label_key.as_ref().is_none_or(|k| k.is_empty()) {
                                        ui.heading(
                                            widget_text_from_key(
                                                w.label_key.as_deref(),
                                                doc,
                                                language,
                                            )
                                            .unwrap_or_else(|| "Aperçu".into()),
                                        );
                                    }
                                    let empty = widget_text_from_key(
                                        w.empty_label_key.as_deref(),
                                        doc,
                                        language,
                                    )
                                    .unwrap_or_else(|| t.decl_preview_empty.to_string());
                                    ui.weak(empty);
                                    ui.add_space(48.0);
                                });
                            }
                        });
                    },
                );
            }
            "layer_canvas" => {
                let canvas_id = w
                    .canvas_id
                    .clone()
                    .or_else(|| w.layers_key.clone())
                    .unwrap_or_else(|| "layer_canvas".into());
                let host = layer_canvases.entry(canvas_id.clone()).or_default();
                let background_path = w
                    .binding
                    .as_ref()
                    .map(|_| image_view_path(w, cache, binding_cache, local_state))
                    .filter(|path| !path.is_empty());
                let opacity_key = w.state_key.as_deref();
                let mut layer_opacity = opacity_key
                    .and_then(|key| local_state.get(key))
                    .and_then(Value::as_f64)
                    .map(|value| value as f32)
                    .unwrap_or(0.72)
                    .clamp(0.0, 1.0);
                if let Some(key) = opacity_key {
                    ui.horizontal(|ui| {
                        let label =
                            widget_text_from_key(Some("layer_opacity_label"), doc, language)
                                .unwrap_or_else(|| "Layer opacity".into());
                        let response = ui.label(label);
                        if let Some(tip) = widget_tooltip(w, doc, language) {
                            response.on_hover_text(tip);
                        }
                        if ui
                            .add(egui::Slider::new(&mut layer_opacity, 0.0..=1.0).show_value(true))
                            .changed()
                        {
                            actions
                                .local_patch
                                .insert(key.to_string(), Value::from(layer_opacity));
                        }
                    });
                }
                let aspect_override = w
                    .aspect_w_key
                    .as_deref()
                    .and_then(|key| local_state.get(key))
                    .and_then(Value::as_u64)
                    .and_then(|width| {
                        w.aspect_h_key
                            .as_deref()
                            .and_then(|key| local_state.get(key))
                            .and_then(Value::as_u64)
                            .map(|height| (width as u32, height as u32))
                    });
                let media_mode = local_state
                    .get("media_mode")
                    .and_then(Value::as_str)
                    .unwrap_or("image");
                let empty_key = if media_mode == "video" {
                    "preview_empty_video"
                } else {
                    "preview_empty"
                };
                let mut canvas_widget = w.clone();
                canvas_widget.empty_label_key = Some(empty_key.into());
                if let Some(patch) = crate::rich_composition_ui::ui_layer_canvas(
                    ui,
                    &canvas_widget,
                    doc,
                    language,
                    local_state,
                    host,
                    &canvas_id,
                    background_path.as_deref(),
                    layer_opacity,
                    aspect_override,
                ) {
                    for (k, v) in patch_to_local_map(&patch) {
                        actions.local_patch.insert(k, v);
                    }
                }
            }
            "layer_list" => {
                let list_id = w
                    .canvas_id
                    .clone()
                    .or_else(|| w.layers_key.clone())
                    .unwrap_or_else(|| "layer_list".into());
                let host = layer_canvases.entry(list_id).or_default();
                if let Some(patch) = crate::rich_composition_ui::ui_layer_list(
                    ui,
                    w,
                    doc,
                    language,
                    local_state,
                    host,
                ) {
                    for (k, v) in patch_to_local_map(&patch) {
                        actions.local_patch.insert(k, v);
                    }
                }
            }
            "undo_redo" => {
                let canvas_id = w.canvas_id.clone().unwrap_or_else(|| "layer_canvas".into());
                if w.scene_key.as_ref().is_some_and(|k| !k.is_empty()) {
                    let host = scene3d_viewports.entry(canvas_id).or_default();
                    if let Some(patch) = crate::scene3d_ui::ui_scene_undo_redo(
                        ui,
                        w,
                        doc,
                        language,
                        local_state,
                        host,
                    ) {
                        for (k, v) in scene_patch_to_local_map(&patch) {
                            actions.local_patch.insert(k, v);
                        }
                        if patch.request_autosave && actions.invoke.is_none() {
                            let yaml = patch.scene.clone();
                            actions.invoke = Some(DeclUiInvokeAction {
                                tool: "illustration.project.save".into(),
                                args: serde_json::json!({
                                    "project_id": local_state.get("project_id"),
                                    "yaml": yaml
                                }),
                                refresh_binds: vec![],
                                clear_form_keys: vec![],
                            });
                            let fr = language.starts_with("fr");
                            // Status surfaced via panel.status in ui_decl_module after invoke.
                            let _ = fr;
                        }
                    }
                } else {
                    let host = layer_canvases.entry(canvas_id).or_default();
                    if let Some(patch) = crate::rich_composition_ui::ui_undo_redo(
                        ui,
                        w,
                        doc,
                        language,
                        local_state,
                        host,
                    ) {
                        for (k, v) in patch_to_local_map(&patch) {
                            actions.local_patch.insert(k, v);
                        }
                    }
                }
            }

            "scene3d" | "illustration_stage" => {
                let viewport_id = w
                    .canvas_id
                    .clone()
                    .or_else(|| w.scene_key.clone())
                    .unwrap_or_else(|| "scene3d".into());
                let host = scene3d_viewports.entry(viewport_id).or_default();
                if w.kind == "illustration_stage" {
                    let project = local_state.get("project_id").and_then(Value::as_str);
                    let render_project = local_state.get("render_project_id").and_then(Value::as_str);
                    let render_kind = local_state.get("render_kind").and_then(Value::as_str);
                    let source_key = if render_kind == Some("comic_render") { "comic" } else { "scene" };
                    let revision_key = if source_key == "comic" { "render_comic_revision" } else { "render_scene_revision" };
                    let revision_matches = local_state.get(source_key).and_then(Value::as_str).is_some_and(|yaml| {
                        use sha2::Digest as _;
                        let actual = format!("{:x}", sha2::Sha256::digest(yaml.as_bytes()));
                        local_state.get(revision_key).and_then(Value::as_str) == Some(actual.as_str())
                    });
                    let path = local_state.get("beauty_path").and_then(Value::as_str).unwrap_or("");
                    if !path.is_empty() && project.is_some() && project == render_project && revision_matches {
                        if let Some(texture) = try_load_png(ui.ctx(), path) {
                            let available = ui.available_size();
                            let base = texture.size_vec2();
                            let fit = (available.x / base.x.max(1.0)).min(available.y / base.y.max(1.0)).max(0.01);
                            let response = ui.add(egui::Image::new(&texture)
                                .fit_to_exact_size(base * fit)
                                .sense(egui::Sense::click()));
                            if response.clicked() {
                                if let Some(pointer) = response.interact_pointer_pos() {
                                    let x = ((pointer.x - response.rect.left()) / response.rect.width()).clamp(0.0, 0.999_999);
                                    let y = ((pointer.y - response.rect.top()) / response.rect.height()).clamp(0.0, 0.999_999);
                                    if render_kind == Some("comic_render") {
                                        if let (Some(yaml), Some(page_id)) = (
                                            local_state.get("comic").and_then(Value::as_str),
                                            local_state.get("render_page_id").and_then(Value::as_str),
                                        ) {
                                            if let Some((panel_id, scene_yaml)) = comic_panel_at(yaml, page_id, x, y) {
                                                actions.local_patch.insert("comic_panel_id".into(), Value::String(panel_id));
                                                actions.local_patch.insert("scene".into(), Value::String(scene_yaml));
                                                actions.local_patch.insert("selected_id".into(), Value::Null);
                                                host.selection.clear();
                                            }
                                        }
                                    } else if let Some(id) = render_node_at(local_state, x, y) {
                                        host.selection = vec![id.clone()];
                                        actions.local_patch.insert("selected_id".into(), Value::String(id));
                                    }
                                    actions.local_patch.insert("beauty_path".into(), Value::String(String::new()));
                                }
                            }
                            return;
                        }
                    }
                }
                if let Some(patch) =
                    crate::scene3d_ui::ui_scene3d(ui, w, doc, language, local_state, host)
                {
                    let autosave = patch.request_autosave;
                    let yaml = patch.scene.clone();
                    for (k, v) in scene_patch_to_local_map(&patch) {
                        actions.local_patch.insert(k, v);
                    }
                    actions.local_patch.insert("beauty_path".into(), Value::String(String::new()));
                    if autosave {
                        if actions.invoke.is_none() {
                            actions.invoke = Some(DeclUiInvokeAction {
                                tool: "illustration.project.save".into(),
                                args: serde_json::json!({
                                    "project_id": local_state.get("project_id"),
                                    "yaml": yaml
                                }),
                                refresh_binds: vec![],
                                clear_form_keys: vec![],
                            });
                        } else {
                            // Busy with another invoke — retry after debounce.
                            host.mark_autosave_dirty();
                        }
                    }
                }
            }
            "scene_tree" => {
                let canvas_id = w
                    .canvas_id
                    .clone()
                    .unwrap_or_else(|| "illustration_viewport".into());
                let host = scene3d_viewports.entry(canvas_id).or_default();
                if let Some(patch) =
                    crate::scene3d_ui::ui_scene_tree(ui, w, doc, language, local_state, host)
                {
                    for (k, v) in scene_patch_to_local_map(&patch) {
                        actions.local_patch.insert(k, v);
                    }
                }
            }
            "scene_object_tools" => {
                let canvas_id = w
                    .canvas_id
                    .clone()
                    .unwrap_or_else(|| "illustration_viewport".into());
                let host = scene3d_viewports.entry(canvas_id).or_default();
                if let Some(patch) =
                    crate::scene3d_ui::ui_scene_object_tools(ui, w, language, local_state, host)
                {
                    for (k, v) in scene_patch_to_local_map(&patch) {
                        actions.local_patch.insert(k, v);
                    }
                }
            }
            "scene_material_editor" => {
                let canvas_id = w
                    .canvas_id
                    .clone()
                    .unwrap_or_else(|| "illustration_viewport".into());
                let host = scene3d_viewports.entry(canvas_id).or_default();
                if let Some(patch) =
                    crate::scene3d_ui::ui_scene_material_editor(ui, w, language, local_state, host)
                {
                    for (k, v) in scene_patch_to_local_map(&patch) {
                        actions.local_patch.insert(k, v);
                    }
                }
            }
            "scene_asset_palette" => {
                crate::scene3d_ui::ui_scene_asset_palette(ui, language);
            }
            "illustration_asset_library" => {
                if w.state_key.as_deref() == Some("scene") {
                    if let Some(assets) = local_state.get("project_assets").and_then(Value::as_array) {
                        for asset in assets.iter().filter(|asset| asset.get("kind").and_then(Value::as_str) == Some("mesh")) {
                            let id = asset.get("id").and_then(Value::as_str).unwrap_or("");
                            let name = asset.get("name").and_then(Value::as_str).unwrap_or("");
                            let project_id = asset.get("project_id").and_then(Value::as_str).unwrap_or("");
                            ui.horizontal(|ui| {
                                ui.label(name);
                                if ui.add_enabled(!pending_invoke, egui::Button::new(library_label(doc, language, "library_add_to_scene"))).clicked() {
                                    queue_invoke(actions, "illustration.asset.add",
                                        serde_json::json!({"project_id": project_id, "asset_id": id}),
                                        Vec::new(), Vec::new(), tool_schemas);
                                }
                            });
                        }
                    }
                } else {
                    render_illustration_asset_library(
                        ui, doc, language, local_state, pending_invoke, actions, tool_schemas,
                    );
                }
            }
            "illustration_library_feedback" => {
                if local_state.get("library_busy").and_then(Value::as_bool) == Some(true) {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(widget_text_from_key(Some("library_operation_wait"), doc, language)
                            .unwrap_or_else(|| "Working…".into()));
                    });
                    ui.add(egui::ProgressBar::new(0.0).animate(true));
                }
                if let Some(message) = local_state.get("library_error").and_then(Value::as_str).filter(|s| !s.is_empty()) {
                    ui.colored_label(egui::Color32::from_rgb(240, 145, 130), message);
                }
            }
            "scene_candidate" => {
                if let Some(key) = w.scene_key.as_deref() {
                    if let Some(yaml) = local_state.get(key).and_then(Value::as_str) {
                        if let Ok(project) = aos_scene::load_project_yaml(yaml) {
                            ui.group(|ui| {
                                ui.strong(
                                    widget_text(w, doc, language)
                                        .unwrap_or_else(|| "Proposal".into()),
                                );
                                let mut roots: Vec<_> = project
                                    .scene
                                    .nodes
                                    .values()
                                    .filter(|node| node.parent.as_deref() == Some("root"))
                                    .filter(|node| {
                                        !matches!(
                                            node.kind,
                                            aos_scene::NodeKind::Camera
                                                | aos_scene::NodeKind::Light
                                        )
                                    })
                                    .collect();
                                roots.sort_by(|a, b| a.id.cmp(&b.id));
                                for node in roots.iter().take(12) {
                                    ui.label(format!(
                                        "{} · x {:.1} m · z {:.1} m",
                                        node.name,
                                        node.transform.translation.x,
                                        node.transform.translation.z
                                    ));
                                }
                                if roots.len() > 12 {
                                    ui.weak(format!("+{}", roots.len() - 12));
                                }
                            });
                        }
                    }
                }
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
    if action.id.starts_with("library_") {
        actions.local_patch.insert("library_error".into(), Value::String(String::new()));
        if action.id != "library_generate_image" {
            actions.local_patch.insert("library_busy".into(), Value::Bool(true));
        } else {
            actions.local_patch.insert("library_job_id".into(), Value::String(String::new()));
            actions.local_patch.insert("library_job_project_id".into(),
                local_state.get("project_id").cloned().unwrap_or(Value::String(String::new())));
        }
    }
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
    if local_state
        .get("preview_cleared")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return String::new();
    }
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
    out.sort_by_key(|a| a.key.clone());
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
    column_label_keys: Option<&[String]>,
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
                for (i, c) in cols.iter().enumerate() {
                    let header = column_label_keys
                        .and_then(|keys| keys.get(i))
                        .and_then(|key| widget_text_from_key(Some(key), doc, language))
                        .unwrap_or_else(|| c.clone());
                    ui.strong(header);
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
                                    let icon = action
                                        .icon_key
                                        .as_deref()
                                        .and_then(icons::resolve_decl_action_icon)
                                        .or_else(|| {
                                            if action.tool == "create.history.get" {
                                                Some(icons::DeclActionIcon::History)
                                            } else {
                                                None
                                            }
                                        });
                                    let clicked = if let Some(icon) = icon {
                                        icons::decl_action_button(
                                            ui, icon, &label, &label, enabled, false,
                                        )
                                    } else {
                                        ui.add_enabled(enabled, egui::Button::new(label)).clicked()
                                    };
                                    if clicked {
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

fn catalog_row_label(
    binding_cache: &HashMap<String, Value>,
    binding_id: &str,
    wire_id: &str,
) -> Option<String> {
    let root = binding_cache.get(binding_id)?;
    for mode in ["image", "video"] {
        if let Some(rows) = root.get(mode).and_then(Value::as_array) {
            for row in rows {
                if row.get("id").and_then(Value::as_str) == Some(wire_id) {
                    return row.get("label").and_then(Value::as_str).map(String::from);
                }
            }
        }
    }
    None
}

fn resolve_catalog_selected_label(
    items: &[String],
    item_labels: &[String],
    current: &str,
    binding_cache: &HashMap<String, Value>,
    binding_id: Option<&str>,
) -> String {
    items
        .iter()
        .position(|item| item == current)
        .and_then(|index| item_labels.get(index))
        .cloned()
        .or_else(|| binding_id.and_then(|id| catalog_row_label(binding_cache, id, current)))
        .unwrap_or_else(|| "—".to_string())
}

fn render_choice(
    ui: &mut Ui,
    w: &DeclUiWidget,
    doc: &DeclUiDocument,
    language: &str,
    binding_cache: &HashMap<String, Value>,
    local_state: &HashMap<String, Value>,
    form_fields: &mut HashMap<String, String>,
    actions: &mut DeclUiActions,
    enabled: bool,
    radio: bool,
) {
    let state_key = w.state_key.clone();
    let key = state_key
        .clone()
        .or_else(|| w.label.clone())
        .or_else(|| w.text.clone())
        .unwrap_or_else(|| "choice".into());
    let dynamic_items = w.binding.as_ref().and_then(|binding_id| {
        let key = w
            .items_from_key
            .as_deref()
            .and_then(|key| key.strip_prefix("$local."))
            .and_then(|key| local_state.get(key).and_then(Value::as_str));
        let value = binding_cache
            .get(binding_id)
            .cloned()
            .unwrap_or(Value::Null);
        let value = key
            .and_then(|key| value.get(key).cloned())
            .or_else(|| {
                w.source
                    .as_deref()
                    .and_then(|source| value.pointer(source).cloned())
            })
            .unwrap_or(value);
        value.as_array().map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    row.as_str()
                        .map(|s| (s.to_string(), s.to_string()))
                        .or_else(|| {
                            row.get("id").and_then(Value::as_str).map(|id| {
                                let label = row.get("label").and_then(Value::as_str).unwrap_or(id);
                                (id.to_string(), label.to_string())
                            })
                        })
                })
                .collect::<Vec<_>>()
        })
    });
    let items = dynamic_items
        .as_ref()
        .map(|rows| rows.iter().map(|(value, _)| value.clone()).collect())
        .unwrap_or_else(|| w.items.clone().unwrap_or_default());
    let item_labels: Vec<String> = dynamic_items
        .as_ref()
        .map(|rows| rows.iter().map(|(_, label)| label.clone()).collect())
        .unwrap_or_else(|| {
            items
                .iter()
                .enumerate()
                .map(|(index, item)| {
                    w.item_label_keys
                        .as_ref()
                        .and_then(|keys| keys.get(index))
                        .and_then(|key| widget_text_from_key(Some(key), doc, language))
                        .unwrap_or_else(|| item.clone())
                })
                .collect()
        });
    let label = widget_text(w, doc, language).unwrap_or_else(|| key.clone());
    let tooltip = widget_tooltip(w, doc, language);
    let has_explicit_label = w.label.is_some() || w.text.is_some() || w.label_key.is_some();
    if let Some(state_key) = state_key {
        let mut current = local_state
            .get(&state_key)
            .and_then(Value::as_str)
            .unwrap_or_else(|| items.first().map(String::as_str).unwrap_or_default())
            .to_string();
        // Dynamic catalogue rows may carry an `installed` flag supplied by
        // the host. Publish a generic readiness value for declarative actions
        // (Create uses it to disable generation of unavailable packs).
        if state_key == "model_id" {
            let mode = local_state
                .get("media_mode")
                .and_then(Value::as_str)
                .unwrap_or("image");
            if !items.iter().any(|item| item == &current) {
                let default_key = if mode == "video" {
                    "default_video"
                } else {
                    "default_image"
                };
                if let Some(binding) = w.binding.as_deref() {
                    if let Some(root) = binding_cache.get(binding) {
                        let replacement = root
                            .get(default_key)
                            .and_then(Value::as_str)
                            .filter(|id| items.iter().any(|item| item == id))
                            .or_else(|| items.first().map(String::as_str));
                        if let Some(id) = replacement {
                            current = id.to_string();
                            actions
                                .local_patch
                                .insert(state_key.clone(), Value::String(current.clone()));
                        }
                    }
                }
            }
            let ready = w
                .binding
                .as_deref()
                .and_then(|binding| binding_cache.get(binding))
                .and_then(|value| value.get(mode))
                .and_then(Value::as_array)
                .and_then(|rows| {
                    rows.iter()
                        .find(|row| row.get("id").and_then(Value::as_str) == Some(current.as_str()))
                })
                .and_then(|row| row.get("installed"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            actions
                .local_patch
                .insert("model_ready".into(), Value::Bool(ready));
        }
        if radio {
            let mut render_item = |ui: &mut Ui, index: usize, item: &String| {
                let changed = if w.inline.unwrap_or(false) {
                    ui.add_enabled(
                        enabled,
                        egui::Button::new(&item_labels[index]).selected(current == *item),
                    )
                    .clicked()
                } else {
                    ui.add_enabled(
                        enabled,
                        egui::RadioButton::new(current == *item, &item_labels[index]),
                    )
                    .clicked()
                };
                if changed {
                    current = item.clone();
                    actions
                        .local_patch
                        .insert(state_key.clone(), Value::String(current.clone()));
                    if state_key == "format" {
                        patch_format_dimensions(local_state, actions, &current);
                    }
                }
            };
            if w.inline.unwrap_or(false) {
                ui.horizontal(|ui| {
                    if has_explicit_label {
                        ui.label(&label);
                    }
                    for (index, item) in items.iter().enumerate() {
                        render_item(ui, index, item);
                    }
                });
            } else {
                ui.label(label);
                for (index, item) in items.iter().enumerate() {
                    render_item(ui, index, item);
                }
            }
        } else {
            ui.add_enabled_ui(enabled, |ui| {
                let response = ui.label(&label);
                if let Some(tip) = tooltip.as_deref() {
                    response.on_hover_text(tip);
                }
                let selected_label = resolve_catalog_selected_label(
                    &items,
                    &item_labels,
                    &current,
                    binding_cache,
                    w.binding.as_deref(),
                );
                egui::ComboBox::from_id_salt(format!("select-{key}"))
                    .selected_text(selected_label)
                    .show_ui(ui, |ui| {
                        for (index, item) in items.iter().enumerate() {
                            if ui
                                .selectable_value(&mut current, item.clone(), &item_labels[index])
                                .changed()
                            {
                                actions
                                    .local_patch
                                    .insert(state_key.clone(), Value::String(current.clone()));
                                if state_key == "format" {
                                    patch_format_dimensions(local_state, actions, &current);
                                }
                            }
                        }
                    });
            });
        }
        return;
    }
    form_fields
        .entry(key.clone())
        .or_insert_with(|| items.first().cloned().unwrap_or_default());
    if radio {
        let mut render_item = |ui: &mut Ui, index: usize, item: &String| {
            ui.add_enabled_ui(enabled, |ui| {
                if w.inline.unwrap_or(false) {
                    let current = form_fields.get(&key).cloned().unwrap_or_default();
                    if ui
                        .add(egui::Button::new(&item_labels[index]).selected(current == *item))
                        .clicked()
                    {
                        form_fields.insert(key.clone(), item.clone());
                    }
                } else {
                    ui.radio_value(
                        form_fields.get_mut(&key).unwrap(),
                        item.clone(),
                        &item_labels[index],
                    );
                }
            });
        };
        if w.inline.unwrap_or(false) {
            ui.horizontal(|ui| {
                if has_explicit_label {
                    let response = ui.label(&label);
                    if let Some(tip) = tooltip.as_deref() {
                        response.on_hover_text(tip);
                    }
                }
                for (index, item) in items.iter().enumerate() {
                    render_item(ui, index, item);
                }
            });
        } else {
            ui.label(label);
            for (index, item) in items.iter().enumerate() {
                render_item(ui, index, item);
            }
        }
    } else {
        let cur = form_fields.get(&key).cloned().unwrap_or_default();
        ui.add_enabled_ui(enabled, |ui| {
            ui.label(&label);
            let selected_label = resolve_catalog_selected_label(
                &items,
                &item_labels,
                &cur,
                binding_cache,
                w.binding.as_deref(),
            );
            egui::ComboBox::from_id_salt(format!("select-{key}"))
                .selected_text(selected_label)
                .show_ui(ui, |ui| {
                    for (index, item) in items.iter().enumerate() {
                        ui.selectable_value(
                            form_fields.get_mut(&key).unwrap(),
                            item.clone(),
                            &item_labels[index],
                        );
                    }
                });
        });
    }
}

fn patch_format_dimensions(
    local_state: &HashMap<String, Value>,
    actions: &mut DeclUiActions,
    format: &str,
) {
    if format == "custom" {
        return;
    }
    let base = local_state
        .get("width")
        .and_then(Value::as_u64)
        .unwrap_or(512)
        .max(
            local_state
                .get("height")
                .and_then(Value::as_u64)
                .unwrap_or(512),
        )
        .clamp(256, 2048);
    let (width, height) = match format {
        "16:9" => (base, (base * 9 / 16).max(64)),
        "9:16" => ((base * 9 / 16).max(64), base),
        "1:1" => (base, base),
        _ => return,
    };
    actions
        .local_patch
        .insert("width".into(), Value::from(width));
    actions
        .local_patch
        .insert("height".into(), Value::from(height));
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
    if let Some(name) = logical.strip_prefix("/assets/illustration/catalogue/thumbs/") {
        if !name.is_empty()
            && name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'-' | b'_'))
        {
            return crate::os_open::aos_home()
                .join("share/assets/illustration/catalogue/thumbs")
                .join(name);
        }
    }
    if let Ok(home) = std::env::var("AOS_HOME") {
        let rel = logical.trim_start_matches('/');
        return std::path::PathBuf::from(home)
            .join("var/storage/data")
            .join(rel);
    }
    std::path::PathBuf::from(logical)
}

/// Import a user-selected image into the module's logical downloads space.
/// Declarative modules never receive an arbitrary host filesystem path.
fn import_decl_media_file(path: &std::path::Path) -> Option<String> {
    if !path.is_file() {
        return None;
    }
    let name = path.file_name()?.to_str()?.to_string();
    let safe_name: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis();
    let filename = format!("create-ref-{stamp}-{safe_name}");
    let logical = format!("/downloads/{filename}");
    let target = host_file_from_logical(&logical);
    std::fs::create_dir_all(target.parent()?).ok()?;
    std::fs::copy(path, &target).ok()?;
    Some(logical)
}

fn widget_tooltip(w: &DeclUiWidget, doc: &DeclUiDocument, language: &str) -> Option<String> {
    widget_text_from_key(w.tooltip_key.as_deref(), doc, language)
}

fn import_decl_asset(path: &std::path::Path, kind: &str) -> Result<String, String> {
    if !path.is_file() {
        return Err("fichier introuvable".into());
    }
    let ext = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let allowed: &[&str] = if kind == "style" {
        &["txt"]
    } else {
        &["safetensors", "ckpt", "pt", "bin"]
    };
    if !allowed.iter().any(|candidate| *candidate == ext) {
        return Err(format!("extension .{ext} non prise en charge pour {kind}"));
    }
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "nom de fichier invalide".to_string())?;
    let safe_name: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let root = crate::os_open::aos_home().join("share/models");
    let folder = match kind {
        "style" => "styles",
        "vae" => "vae",
        _ => "lora",
    };
    let target_dir = root.join(folder);
    std::fs::create_dir_all(&target_dir).map_err(|error| error.to_string())?;
    let mut target = target_dir.join(&safe_name);
    if target.exists() {
        let stem = target
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("asset");
        target = target_dir.join(format!("{stem}-imported.{ext}"));
    }
    std::fs::copy(path, &target).map_err(|error| error.to_string())?;
    Ok(target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(&safe_name)
        .to_string())
}

fn add_decl_custom_style(style: &str) -> Result<String, String> {
    let value = style.trim();
    if value.is_empty() {
        return Err("style vide".into());
    }
    let path = crate::os_open::aos_home().join("var/run/image-assets.json");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut registry = std::fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .unwrap_or_else(|| serde_json::json!({"styles": [], "loras": [], "vaes": []}));
    let styles = registry
        .as_object_mut()
        .ok_or_else(|| "registre d'assets invalide".to_string())?
        .entry("styles")
        .or_insert_with(|| Value::Array(Vec::new()));
    let list = styles
        .as_array_mut()
        .ok_or_else(|| "liste de styles invalide".to_string())?;
    if !list.iter().any(|item| item.as_str() == Some(value)) {
        list.push(Value::String(value.to_string()));
    }
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&registry).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    Ok(value.to_string())
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

#[derive(Clone)]
struct LibraryOrbit {
    yaw: f32,
    pitch: f32,
    zoom: f32,
}

impl Default for LibraryOrbit {
    fn default() -> Self {
        Self { yaw: 0.65, pitch: 0.35, zoom: 1.0 }
    }
}

fn library_label(doc: &DeclUiDocument, language: &str, key: &str) -> String {
    widget_text_from_key(Some(key), doc, language).unwrap_or_else(|| key.to_owned())
}

fn library_asset_texture(ctx: &egui::Context, asset: &Value, project_id: &str) -> Option<egui::TextureHandle> {
    let uri = asset.get("uri")?.as_str()?;
    let kind = asset.get("kind")?.as_str()?;
    let thumbnail = if kind == "image" {
        uri
    } else {
        asset.get("metadata")?.get("thumbnail")?.as_str()?
    };
    if thumbnail.contains("..")
        || !(thumbnail.starts_with("/assets/illustration/")
            || thumbnail.starts_with(&format!("/documents/illustrations/projects/{project_id}/assets/")))
    {
        return None;
    }
    let key = egui::Id::new(("illustration-library-thumb", project_id, thumbnail));
    ctx.data(|data| data.get_temp::<egui::TextureHandle>(key)).or_else(|| {
        let loaded = try_load_asset_thumbnail(ctx, thumbnail)?;
        ctx.data_mut(|data| data.insert_temp(key, loaded.clone()));
        Some(loaded)
    })
}

fn library_mesh_texture(
    ctx: &egui::Context,
    asset: &Value,
    yaw: f32,
    pitch: f32,
    zoom: f32,
    size: [u32; 2],
) -> Result<egui::TextureHandle, String> {
    let uri = asset.get("uri").and_then(Value::as_str).ok_or("Missing GLB path")?;
    if !uri.starts_with("/documents/illustrations/") || uri.contains("..") || uri.contains('\\') {
        return Err("Invalid GLB path".into());
    }
    let path = host_file_from_logical(uri);
    let mesh = aos_scene::load_gltf_mesh(&path).map_err(|error| error.to_string())?;
    let target = mesh.aabb_center();
    let extent = mesh.aabb_half_extents();
    let radius = (extent.x * extent.x + extent.y * extent.y + extent.z * extent.z)
        .sqrt().max(0.25);
    let mut graph = aos_scene::SceneGraph {
        effects: Vec::new(),
        nodes: Default::default(),
        roots: vec!["root".into()],
        active_camera: None,
    };
    graph.nodes.insert("root".into(), aos_scene::SceneNode::empty("root", "Asset preview"));
    aos_scene::insert_mesh_asset(
        &mut graph,
        "root",
        "preview_mesh",
        "Asset",
        path.to_string_lossy().into_owned(),
        aos_scene::Transform::default(),
    ).map_err(|error| error.to_string())?;
    let camera = aos_scene::ViewportCamera {
        eye: aos_scene::eye_from_orbit(yaw, pitch, radius * 3.1 * zoom, target),
        target,
        up: aos_scene::Vec3::UNIT_Y,
        fovy_rad: 0.72,
        near: (radius * 0.01).max(0.001),
        far: (radius * 12.0).max(20.0),
    };
    let gpu = crate::scene3d_ui::scene_gpu().ok_or("3D preview unavailable")?;
    let rgba = gpu.lock().map_err(|error| error.to_string())?
        .render_rgba(&graph, &camera, size[0], size[1], None)
        .map_err(|error| error.to_string())?;
    let image = egui::ColorImage::from_rgba_unmultiplied(
        [size[0] as usize, size[1] as usize], &rgba,
    );
    Ok(ctx.load_texture(
        format!("illustration-library-mesh-{}", asset.get("id").and_then(Value::as_str).unwrap_or("asset")),
        image,
        egui::TextureOptions::LINEAR,
    ))
}

fn render_illustration_asset_library(
    ui: &mut Ui,
    doc: &DeclUiDocument,
    language: &str,
    local_state: &HashMap<String, Value>,
    pending_invoke: bool,
    actions: &mut DeclUiActions,
    tool_schemas: &HashMap<String, Value>,
) {
    let project_id = local_state.get("project_id").and_then(Value::as_str).unwrap_or("");
    let assets = local_state.get("project_assets").and_then(Value::as_array);
    let selected_id = if local_state.get("library_selected_project_id").and_then(Value::as_str) == Some(project_id) {
        local_state.get("library_selected_asset_id").and_then(Value::as_str).unwrap_or("")
    } else {
        ""
    };
    let selected = assets.and_then(|items| library_selected_asset(
        items,
        project_id,
        local_state.get("library_selected_project_id").and_then(Value::as_str).unwrap_or(""),
        selected_id,
    ));
    let wide = ui.available_width() >= 760.0;
    let height = ui.available_height().max(360.0);
    if wide {
        ui.horizontal(|ui| {
            ui.set_min_height(height);
            let grid_width = ui.available_width() * 0.58;
            ui.allocate_ui_with_layout(
                egui::vec2(grid_width, height),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    ui.set_min_height(height);
                    render_library_grid(ui, doc, language, assets, project_id, selected_id, actions);
                },
            );
            ui.separator();
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), height),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    ui.set_min_height(height);
                    egui::ScrollArea::vertical().id_salt("library-inspector").show(ui, |ui| {
                        render_library_details(ui, doc, language, selected, project_id, pending_invoke, actions, tool_schemas);
                    });
                },
            );
        });
    } else {
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), height.min(340.0)),
            egui::Layout::top_down(egui::Align::LEFT),
            |ui| render_library_grid(ui, doc, language, assets, project_id, selected_id, actions),
        );
        ui.separator();
        egui::ScrollArea::vertical().id_salt("library-inspector-compact").show(ui, |ui| {
            render_library_details(ui, doc, language, selected, project_id, pending_invoke, actions, tool_schemas);
        });
    }
}

fn library_image_detail_texture(ctx: &egui::Context, logical: &str) -> Option<egui::TextureHandle> {
    if !logical.starts_with("/documents/illustrations/") || logical.contains("..") || logical.contains('\\') {
        return None;
    }
    let path = host_file_from_logical(logical);
    if std::fs::metadata(&path).ok()?.len() > 100_000_000 {
        return None;
    }
    let image = image::open(path).ok()?.thumbnail(2048, 2048).to_rgba8();
    let color = egui::ColorImage::from_rgba_unmultiplied(
        [image.width() as usize, image.height() as usize],
        image.as_raw(),
    );
    Some(ctx.load_texture(logical, color, egui::TextureOptions::LINEAR))
}

fn library_selected_asset<'a>(
    assets: &'a [Value],
    project_id: &str,
    selected_project_id: &str,
    selected_id: &str,
) -> Option<&'a Value> {
    if project_id != selected_project_id || selected_id.is_empty() {
        return None;
    }
    assets.iter().find(|asset| {
        asset.get("project_id").and_then(Value::as_str) == Some(project_id)
            && asset.get("id").and_then(Value::as_str) == Some(selected_id)
    })
}

fn render_library_grid(
    ui: &mut Ui,
    doc: &DeclUiDocument,
    language: &str,
    assets: Option<&Vec<Value>>,
    project_id: &str,
    selected_id: &str,
    actions: &mut DeclUiActions,
) {
    ui.heading(library_label(doc, language, "library_heading"));
    let Some(assets) = assets.filter(|assets| !assets.is_empty()) else {
        ui.weak(library_label(doc, language, "library_empty"));
        return;
    };
    let columns = ((ui.available_width() / 166.0).floor() as usize).max(1);
    egui::ScrollArea::vertical().id_salt("library-asset-grid").show(ui, |ui| {
        egui::Grid::new("library-assets").spacing([10.0, 10.0]).show(ui, |ui| {
            for (index, asset) in assets.iter().enumerate() {
                let id = asset.get("id").and_then(Value::as_str).unwrap_or("");
                let name = asset.get("name").and_then(Value::as_str).unwrap_or("");
                let is_image = asset.get("kind").and_then(Value::as_str) == Some("image");
                let frame = egui::Frame::group(ui.style())
                    .stroke(if id == selected_id {
                        egui::Stroke::new(2.0, ui.visuals().selection.stroke.color)
                    } else {
                        ui.visuals().widgets.noninteractive.bg_stroke
                    });
                let response = frame.show(ui, |ui| {
                    ui.set_min_width(145.0);
                    ui.set_max_width(145.0);
                    ui.set_min_height(178.0);
                    ui.vertical(|ui| {
                    let (preview, _) = ui.allocate_exact_size(egui::vec2(137.0, 112.0), egui::Sense::hover());
                    if let Some(texture) = library_asset_texture(ui.ctx(), asset, project_id) {
                        let size = texture.size_vec2();
                        let scale = (preview.width() / size.x).min(preview.height() / size.y);
                        let draw = egui::Rect::from_center_size(preview.center(), size * scale);
                        ui.painter().image(texture.id(), draw, egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
                    } else if !is_image {
                        let key = ui.id().with(("library-mesh-thumb", project_id, id));
                        let texture = ui.ctx().data(|data| data.get_temp::<egui::TextureHandle>(key))
                            .or_else(|| {
                                let rendered = library_mesh_texture(ui.ctx(), asset, 0.65, 0.35, 1.0, [160, 144]).ok()?;
                                ui.ctx().data_mut(|data| data.insert_temp(key, rendered.clone()));
                                Some(rendered)
                            });
                        if let Some(texture) = texture {
                            ui.painter().image(texture.id(), preview, egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
                        } else {
                            ui.painter().text(preview.center(), egui::Align2::CENTER_CENTER, "3D", egui::FontId::proportional(22.0), ui.visuals().weak_text_color());
                        }
                    }
                    let short_name = name.chars().take(18).collect::<String>();
                    let short_name = if name.chars().count() > 18 { format!("{short_name}…") } else { short_name };
                    ui.label(egui::RichText::new(short_name).strong()).on_hover_text(name);
                    egui::Frame::default()
                        .fill(ui.visuals().faint_bg_color)
                        .corner_radius(3.0)
                        .inner_margin(egui::Margin::symmetric(5, 2))
                        .show(ui, |ui| {
                            ui.weak(library_label(doc, language, if is_image { "library_kind_image" } else { "library_kind_mesh" }));
                        });
                    });
                }).response;
                let clicked = ui.interact(response.rect, ui.id().with(("library-card", project_id, id)), egui::Sense::click());
                if clicked.clicked() || (clicked.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter) || input.key_pressed(egui::Key::Space))) {
                    actions.local_patch.insert("library_selected_asset_id".into(), Value::String(id.into()));
                    actions.local_patch.insert("library_selected_project_id".into(), Value::String(project_id.into()));
                    actions.local_patch.insert("library_image_uri".into(), Value::String(if is_image { asset.get("uri").and_then(Value::as_str).unwrap_or("") } else { "" }.into()));
                    actions.local_patch.insert("library_image_prompt".into(), Value::String(if is_image { asset.get("prompt").and_then(Value::as_str).unwrap_or("") } else { "" }.into()));
                }
                if (index + 1) % columns == 0 {
                    ui.end_row();
                }
            }
        });
    });
}


fn render_library_details(
    ui: &mut Ui,
    doc: &DeclUiDocument,
    language: &str,
    selected: Option<&Value>,
    project_id: &str,
    pending_invoke: bool,
    actions: &mut DeclUiActions,
    tool_schemas: &HashMap<String, Value>,
) {
    ui.heading(library_label(doc, language, "library_details"));
    let Some(asset) = selected else {
        ui.weak(library_label(doc, language, "library_detail_empty"));
        return;
    };
    let id = asset.get("id").and_then(Value::as_str).unwrap_or("");
    let name = asset.get("name").and_then(Value::as_str).unwrap_or("");
    let kind = asset.get("kind").and_then(Value::as_str).unwrap_or("");
    let uri = asset.get("uri").and_then(Value::as_str).unwrap_or("");
    ui.strong(name);
    ui.weak(library_label(doc, language, if kind == "image" { "library_kind_image" } else { "library_kind_mesh" }));
    ui.add_space(8.0);
    if kind == "image" {
        let key = ui.id().with(("library-image-detail", project_id, id));
        let texture = ui.ctx().data(|data| data.get_temp::<egui::TextureHandle>(key))
            .or_else(|| {
                let loaded = library_image_detail_texture(ui.ctx(), uri)?;
                ui.ctx().data_mut(|data| data.insert_temp(key, loaded.clone()));
                Some(loaded)
            });
        if let Some(texture) = texture {
            let size = texture.size_vec2();
            let scale = (ui.available_width() / size.x).min(420.0 / size.y);
            ui.add(egui::Image::new(&texture).fit_to_exact_size(size * scale));
            if let Ok((width, height)) = image::image_dimensions(host_file_from_logical(uri)) {
                ui.weak(format!("{}: {width} × {height} px",
                    if language.starts_with("fr") { "Résolution" } else { "Resolution" }));
            }
        } else {
            ui.weak(library_label(doc, language, "library_preview_unavailable"));
        }
        ui.separator();
        render_library_metadata(ui, asset, language);
    } else if ui.available_width() >= 440.0 {
        ui.horizontal(|ui| {
            let preview_width = ui.available_width() * 0.53;
            ui.allocate_ui_with_layout(
                egui::vec2(preview_width, 385.0),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| render_library_mesh_preview(ui, doc, language, asset, project_id),
            );
            ui.separator();
            ui.vertical(|ui| render_library_metadata(ui, asset, language));
        });
    } else {
        render_library_mesh_preview(ui, doc, language, asset, project_id);
        ui.separator();
        render_library_metadata(ui, asset, language);
    }
    ui.add_space(8.0);
    if kind == "image" {
        if let Some(action) = doc.actions.iter().find(|action| action.id == "library_convert_trellis") {
            if ui.add_enabled(!pending_invoke, egui::Button::new(library_label(doc, language, "library_convert"))).clicked() {
                let mut state = HashMap::new();
                state.insert("project_id".into(), Value::String(project_id.into()));
                state.insert("library_image_uri".into(), Value::String(uri.into()));
                state.insert("library_image_prompt".into(), asset.get("prompt").cloned().unwrap_or(Value::String(String::new())));
                queue_service_action(actions, action, &state, &HashMap::new(), doc);
            }
        }
    } else if kind == "mesh"
        && ui.add_enabled(!pending_invoke, egui::Button::new(library_label(doc, language, "library_add_to_scene"))).clicked()
    {
        queue_invoke(actions, "illustration.asset.add",
            serde_json::json!({"project_id": project_id, "asset_id": id}),
            Vec::new(), Vec::new(), tool_schemas);
    }
}

fn render_library_mesh_preview(
    ui: &mut Ui,
    doc: &DeclUiDocument,
    language: &str,
    asset: &Value,
    project_id: &str,
) {
    let id = asset.get("id").and_then(Value::as_str).unwrap_or("");
    let key = ui.id().with(("library-orbit", project_id, id));
    let mut orbit = ui.ctx().data(|data| data.get_temp::<LibraryOrbit>(key)).unwrap_or_default();
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().max(200.0), 320.0), egui::Sense::click_and_drag(),
    );
    let mut changed = false;
    if response.dragged() {
        let delta = ui.input(|input| input.pointer.delta());
        orbit.yaw += delta.x * 0.01;
        orbit.pitch = (orbit.pitch - delta.y * 0.01).clamp(-1.35, 1.35);
        changed = true;
    }
    if response.hovered() {
        let scroll = ui.input(|input| input.smooth_scroll_delta.y);
        if scroll.abs() > 0.1 {
            orbit.zoom = (orbit.zoom * (1.0 - scroll * 0.002)).clamp(0.35, 5.0);
            changed = true;
        }
    }
    let texture_key = ui.id().with(("library-mesh-detail", project_id, id));
    let mut texture = ui.ctx().data(|data| data.get_temp::<egui::TextureHandle>(texture_key));
    if changed || texture.is_none() {
        texture = library_mesh_texture(ui.ctx(), asset, orbit.yaw, orbit.pitch, orbit.zoom, [512, 384]).ok();
        if let Some(texture) = &texture {
            ui.ctx().data_mut(|data| data.insert_temp(texture_key, texture.clone()));
        }
    }
    if let Some(texture) = texture {
        ui.painter().image(texture.id(), rect, egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
    } else {
        ui.painter().rect_filled(rect, 4.0, ui.visuals().faint_bg_color);
        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER,
            library_label(doc, language, "library_preview_unavailable"),
            egui::FontId::proportional(13.0), ui.visuals().text_color());
    }
    ui.weak(library_label(doc, language, "library_preview_controls"));
    if ui.button(library_label(doc, language, "library_preview_reset")).clicked() {
        orbit = LibraryOrbit::default();
        ui.ctx().data_mut(|data| data.remove::<egui::TextureHandle>(texture_key));
    }
    ui.ctx().data_mut(|data| data.insert_temp(key, orbit));
}

fn render_library_metadata(ui: &mut Ui, asset: &Value, language: &str) {
    let fr = language.starts_with("fr");
    let metadata = &asset["metadata"];
    let missing = if fr { "Non renseigné" } else { "Not specified" };
    for (label, value) in [
        (if fr { "Identifiant" } else { "ID" }, asset.get("id").and_then(Value::as_str)),
        (if fr { "Catégorie" } else { "Category" }, metadata.get("category").and_then(Value::as_str)),
        (if fr { "Origine" } else { "Origin" }, metadata.get("provenance").and_then(Value::as_str)),
        (if fr { "Licence" } else { "License" }, metadata.get("license").and_then(Value::as_str)),
        (if fr { "Auteur" } else { "Author" }, metadata.get("author").and_then(Value::as_str)),
        (if fr { "Modèle" } else { "Model" }, metadata.get("model_id").and_then(Value::as_str)),
        ("Orientation", metadata.get("orientation").and_then(Value::as_str)),
        ("Source", metadata.get("source_url").and_then(Value::as_str)),
        (if fr { "Invite" } else { "Prompt" }, asset.get("prompt").and_then(Value::as_str)),
    ] {
        ui.horizontal_wrapped(|ui| {
            ui.weak(format!("{label}:"));
            ui.label(value.filter(|value| !value.is_empty()).unwrap_or(missing));
        });
    }
    if let Some(values) = metadata.get("dimensions_m").and_then(Value::as_array).filter(|items| items.len() == 3) {
        let dimensions = values.iter().filter_map(Value::as_f64).map(|n| format!("{n:.2}")).collect::<Vec<_>>();
        if dimensions.len() == 3 {
            ui.label(format!("Dimensions: {} × {} × {} m", dimensions[0], dimensions[1], dimensions[2]));
        }
    }
    if let Some(values) = metadata.get("pivot_m").and_then(Value::as_array).filter(|items| items.len() == 3) {
        let pivot = values.iter().filter_map(Value::as_f64).map(|n| format!("{n:.2}")).collect::<Vec<_>>();
        if pivot.len() == 3 {
            ui.label(format!("Pivot: {} × {} × {} m", pivot[0], pivot[1], pivot[2]));
        }
    }
    if let Some(tags) = metadata.get("tags").and_then(Value::as_array) {
        let labels = tags.iter().filter_map(Value::as_str).collect::<Vec<_>>();
        if !labels.is_empty() {
            ui.label(format!("Tags: {}", labels.join(", ")));
        }
    }
}

fn render_node_at(local_state: &HashMap<String, Value>, x: f32, y: f32) -> Option<String> {
    let path = local_state.get("render_id_map_path")?.as_str()?;
    let nodes = local_state.get("render_id_map_nodes")?.as_array()?;
    let image = image::open(host_file_from_logical(path)).ok()?.to_rgba8();
    if image.width() != local_state.get("render_id_map_width")?.as_u64()? as u32
        || image.height() != local_state.get("render_id_map_height")?.as_u64()? as u32
    {
        return None;
    }
    let px = image.get_pixel(
        (x.clamp(0.0, 0.999_999) * image.width() as f32) as u32,
        (y.clamp(0.0, 0.999_999) * image.height() as f32) as u32,
    );
    let index = (px[0] as usize) | ((px[1] as usize) << 8) | ((px[2] as usize) << 16);
    index.checked_sub(1).and_then(|index| nodes.get(index))?.as_str().map(str::to_string)
}

fn comic_panel_at(yaml: &str, page_id: &str, x: f32, y: f32) -> Option<(String, String)> {
    let comic = aos_scene::load_comic_yaml(yaml).ok()?;
    let page = comic.pages.iter().find(|page| page.id == page_id)?;
    page.panels.iter().find(|panel| {
        x >= panel.rect.x && x <= panel.rect.x + panel.rect.w
            && y >= panel.rect.y && y <= panel.rect.y + panel.rect.h
    }).map(|panel| (panel.id.clone(), panel.scene_yaml.clone()))
}

fn try_load_asset_thumbnail(
    ctx: &egui::Context,
    logical: &str,
) -> Option<egui::TextureHandle> {
    let path = host_file_from_logical(logical);
    if std::fs::metadata(&path).ok()?.len() > 100_000_000 {
        return None;
    }
    let img = image::open(path).ok()?.thumbnail(144, 144).to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
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

#[cfg(test)]
mod illustration_library_tests {
    use super::library_selected_asset;
    use serde_json::json;

    #[test]
    fn selection_is_scoped_to_the_active_project() {
        let assets = vec![
            json!({"project_id": "project-1", "id": "asset-1"}),
            json!({"project_id": "project-2", "id": "asset-1"}),
        ];
        assert_eq!(
            library_selected_asset(&assets, "project-2", "project-2", "asset-1"),
            Some(&assets[1])
        );
        assert!(library_selected_asset(&assets, "project-2", "project-1", "asset-1").is_none());
    }
}

#[cfg(test)]
mod illustration_stage_tests {
    use super::{comic_panel_at, render_node_at};

    #[test]
    fn render_click_selects_only_pixels_present_in_id_map() {
        let path = std::env::temp_dir().join(format!(
            "illustration-id-map-{}.png",
            std::process::id()
        ));
        let mut map = image::RgbaImage::from_pixel(2, 1, image::Rgba([0, 0, 0, 255]));
        map.put_pixel(1, 0, image::Rgba([1, 0, 0, 255]));
        map.save(&path).unwrap();
        let state = std::collections::HashMap::from([
            ("render_id_map_path".into(), serde_json::json!(path.to_string_lossy())),
            ("render_id_map_nodes".into(), serde_json::json!(["box"])),
            ("render_id_map_width".into(), serde_json::json!(2)),
            ("render_id_map_height".into(), serde_json::json!(1)),
        ]);
        assert_eq!(render_node_at(&state, 0.75, 0.5).as_deref(), Some("box"));
        assert!(render_node_at(&state, 0.25, 0.5).is_none());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn comic_click_uses_the_panel_rect_and_scene_snapshot() {
        let comic = aos_scene::apply_comic_layout(
            aos_scene::ComicLayoutId::TwoHorizontal,
            &aos_scene::SceneGraph::demo_scene(),
            640,
            480,
        ).unwrap();
        let yaml = aos_scene::save_comic_yaml(&comic).unwrap();
        let left = comic_panel_at(&yaml, "page_1", 0.25, 0.5).unwrap();
        let right = comic_panel_at(&yaml, "page_1", 0.75, 0.5).unwrap();
        assert_eq!(left.0, "panel_1");
        assert_eq!(right.0, "panel_2");
        assert_eq!(left.1, comic.pages[0].panels[0].scene_yaml);
        assert!(comic_panel_at(&yaml, "page_1", 0.5, 0.5).is_none());
    }
}
