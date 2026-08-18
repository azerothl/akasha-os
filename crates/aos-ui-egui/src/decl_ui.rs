//! Host-rendered declarative module UI (E15 / Preview 0.7).

use aos_proto::decl_ui::{DeclUiDocument, DeclUiWidget};
use aos_proto::ModuleTool;
use eframe::egui::{self, Ui};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use egui_plot::{Line, Plot, PlotPoints};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct DeclUiActions {
    pub refresh: bool,
    pub invoke: Option<(String, Value)>,
}

#[derive(Debug, Default)]
pub struct DeclUiPanelState {
    pub module: String,
    pub document: Option<DeclUiDocument>,
    pub error: String,
    pub bind_cache: HashMap<String, Value>,
    pub form_fields: HashMap<String, String>,
    pub status: String,
    pub tool_schemas: HashMap<String, Value>,
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
        self.document = Some(doc);
    }

    pub fn set_error(&mut self, msg: impl Into<String>) {
        self.document = None;
        self.error = msg.into();
    }

    pub fn set_bind_result(&mut self, tool: &str, result: Value) {
        self.bind_cache.insert(tool.to_string(), result);
    }

    pub fn tools_to_bind(&self) -> Vec<String> {
        self.document
            .as_ref()
            .map(|d| d.bind_tools())
            .unwrap_or_default()
    }

    pub fn ui(
        &mut self,
        ui: &mut Ui,
        md_cache: &mut CommonMarkCache,
        refresh_label: &str,
    ) -> DeclUiActions {
        let mut actions = DeclUiActions::default();
        if !self.error.is_empty() {
            ui.colored_label(egui::Color32::RED, &self.error);
            return actions;
        }
        let Some(doc) = self.document.clone() else {
            ui.weak("…");
            return actions;
        };
        ui.horizontal(|ui| {
            ui.heading(&doc.title);
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
                    &self.bind_cache,
                    &mut self.form_fields,
                    &self.tool_schemas,
                    &mut actions,
                );
            }
        } else {
            Self::render_widget(
                ui,
                md_cache,
                &doc.root,
                &self.bind_cache,
                &mut self.form_fields,
                &self.tool_schemas,
                &mut actions,
            );
        }
        actions
    }

    fn render_widget(
        ui: &mut Ui,
        md_cache: &mut CommonMarkCache,
        w: &DeclUiWidget,
        cache: &HashMap<String, Value>,
        form_fields: &mut HashMap<String, String>,
        tool_schemas: &HashMap<String, Value>,
        actions: &mut DeclUiActions,
    ) {
        match w.kind.as_str() {
            "column" => {
                ui.vertical(|ui| {
                    if let Some(children) = &w.children {
                        for c in children {
                            Self::render_widget(
                                ui, md_cache, c, cache, form_fields, tool_schemas, actions,
                            );
                        }
                    }
                });
            }
            "row" => {
                ui.horizontal(|ui| {
                    if let Some(children) = &w.children {
                        for c in children {
                            Self::render_widget(
                                ui, md_cache, c, cache, form_fields, tool_schemas, actions,
                            );
                        }
                    }
                });
            }
            "heading" => {
                if let Some(t) = &w.text {
                    ui.heading(t);
                }
            }
            "text" => {
                if let Some(t) = &w.text {
                    ui.label(t);
                }
            }
            "markdown" => {
                if let Some(t) = &w.text {
                    CommonMarkViewer::new().show(ui, md_cache, t);
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
                    render_table(ui, &val, w.columns.as_deref());
                }
            }
            "line_chart" => {
                if let Some(bind) = &w.bind {
                    let val = resolve_bind(cache, bind, w.source.as_deref());
                    render_line_chart(ui, &val, w.series.as_deref());
                }
            }
            "button" => {
                let label = w
                    .label
                    .as_deref()
                    .or(w.text.as_deref())
                    .unwrap_or("Run");
                if ui.button(label).clicked() {
                    if let Some(tool) = &w.tool {
                        let args = w.args.clone().unwrap_or_else(|| Value::Object(Default::default()));
                        actions.invoke = Some((tool.clone(), args));
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
                    let fields = schema_fields(&schema);
                    ui.group(|ui| {
                        for (key, label) in &fields {
                            form_fields
                                .entry(key.clone())
                                .or_insert_with(String::new);
                            ui.horizontal(|ui| {
                                ui.label(label);
                                ui.text_edit_singleline(form_fields.get_mut(key).unwrap());
                            });
                        }
                        let submit = w.label.as_deref().unwrap_or("Submit");
                        if ui.button(submit).clicked() {
                            let mut args = serde_json::Map::new();
                            for (key, _) in &fields {
                                if let Some(v) = form_fields.get(key) {
                                    args.insert(key.clone(), Value::String(v.clone()));
                                }
                            }
                            actions.invoke = Some((tool.clone(), Value::Object(args)));
                        }
                    });
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

fn schema_fields(schema: &Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Some(props) = schema.get("properties").and_then(|p| p.as_object()) {
        for (k, v) in props {
            let label = v
                .get("title")
                .or_else(|| v.get("description"))
                .and_then(|x| x.as_str())
                .unwrap_or(k.as_str())
                .to_string();
            out.push((k.clone(), label));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn render_stats(ui: &mut Ui, val: &Value, items: Option<&[String]>) {
    match val {
        Value::Object(map) => {
            let keys: Vec<_> = if let Some(items) = items {
                items.iter().cloned().collect()
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

fn render_table(ui: &mut Ui, val: &Value, columns: Option<&[String]>) {
    let rows: Vec<Value> = match val {
        Value::Array(a) => a.clone(),
        Value::Object(o) => o
            .get("items")
            .or_else(|| o.get("rows"))
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    if rows.is_empty() {
        ui.weak("—");
        return;
    }
    let cols: Vec<String> = if let Some(c) = columns {
        c.to_vec()
    } else if let Value::Object(first) = rows.first().unwrap_or(&Value::Null) {
        let mut k: Vec<_> = first.keys().cloned().collect();
        k.sort();
        k
    } else {
        vec!["value".into()]
    };
    egui::Grid::new("decl_ui_table").striped(true).show(ui, |ui| {
        for c in &cols {
            ui.strong(c);
        }
        ui.end_row();
        for row in &rows {
            match row {
                Value::Object(map) => {
                    for c in &cols {
                        ui.label(map.get(c).map(value_display).unwrap_or_else(|| "—".into()));
                    }
                }
                Value::Array(cells) => {
                    for c in cols.iter().enumerate() {
                        let cell = cells.get(c.0).map(value_display).unwrap_or_else(|| "—".into());
                        ui.label(cell);
                    }
                }
                other => {
                    ui.label(value_display(other));
                    for _ in 1..cols.len() {
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
    Plot::new("decl_ui_plot")
        .height(160.0)
        .show(ui, |plot_ui| {
            plot_ui.line(
                Line::new(PlotPoints::from_iter(
                    points.iter().enumerate().map(|(i, y)| [i as f64, *y]),
                ))
                .name(series_key.unwrap_or("series")),
            );
        });
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
