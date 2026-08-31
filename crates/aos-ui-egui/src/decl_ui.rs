//! Host-rendered declarative module UI (E15 / Preview 0.7).

use aos_proto::decl_ui::{DeclUiDocument, DeclUiWidget};
use aos_proto::ModuleTool;
use eframe::egui::{self, Ui};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use egui_plot::{Bar, BarChart, Line, Plot, PlotPoints, Points};
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
            ui.colored_label(
                egui::Color32::RED,
                format!("{}: {}", self.module, self.error),
            );
            if ui.button(refresh_label).clicked() {
                actions.refresh = true;
            }
            return actions;
        }
        let Some(doc) = self.document.clone() else {
            ui.weak(format!("{}…", self.module));
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
                                        ui, md_cache, c, cache, form_fields, tool_schemas, actions,
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
                let mut on = form_fields
                    .get(&key)
                    .map(|s| s == "true")
                    .unwrap_or(false);
                if ui.checkbox(&mut on, &key).changed() {
                    form_fields.insert(key, if on { "true".into() } else { "false".into() });
                }
            }
            "textarea" => {
                let key = w
                    .label
                    .clone()
                    .unwrap_or_else(|| "text".into());
                form_fields.entry(key.clone()).or_default();
                ui.label(&key);
                ui.text_edit_multiline(form_fields.get_mut(&key).unwrap());
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
                        for field in &fields {
                            form_fields
                                .entry(field.key.clone())
                                .or_insert_with(|| field.default_string());
                            ui.horizontal(|ui| {
                                ui.label(&field.label);
                                match &field.kind {
                                    FieldKind::Bool => {
                                        let mut on = form_fields.get(&field.key).map(|s| s == "true").unwrap_or(false);
                                        if ui.checkbox(&mut on, "").changed() {
                                            form_fields.insert(
                                                field.key.clone(),
                                                if on { "true".into() } else { "false".into() },
                                            );
                                        }
                                    }
                                    FieldKind::Enum(vals) => {
                                        let cur = form_fields
                                            .get(&field.key)
                                            .cloned()
                                            .unwrap_or_default();
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
                                        ui.text_edit_multiline(form_fields.get_mut(&field.key).unwrap());
                                    }
                                    FieldKind::Number | FieldKind::Text => {
                                        ui.text_edit_singleline(form_fields.get_mut(&field.key).unwrap());
                                    }
                                }
                            });
                        }
                        let submit = w.label.as_deref().unwrap_or("Submit");
                        if ui.button(submit).clicked() {
                            let mut args = serde_json::Map::new();
                            for field in &fields {
                                if let Some(v) = form_fields.get(&field.key) {
                                    args.insert(field.key.clone(), field.parse_value(v));
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

fn schema_fields(schema: &Value) -> Vec<SchemaField> {
    let mut out = Vec::new();
    if let Some(props) = schema.get("properties").and_then(|p| p.as_object()) {
        for (k, v) in props {
            let label = v
                .get("title")
                .or_else(|| v.get("description"))
                .and_then(|x| x.as_str())
                .unwrap_or(k.as_str())
                .to_string();
            out.push(SchemaField {
                key: k.clone(),
                label,
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
                let long = v
                    .get("maxLength")
                    .and_then(|m| m.as_u64())
                    .unwrap_or(0)
                    > 120;
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

fn render_bar_chart(ui: &mut Ui, val: &Value, series_key: Option<&str>) {
    let points = extract_series_points(val, series_key);
    if points.is_empty() {
        ui.weak("—");
        return;
    }
    Plot::new("decl_ui_bar")
        .height(160.0)
        .show(ui, |plot_ui| {
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
