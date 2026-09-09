//! Host-rendered declarative module UI (E15 / Preview 0.7).
//!
//! Modules ship a JSON widget tree in `ui/index.html` (`type: declarative_ui`).
//! The egui host paints a closed vocabulary — no HTML/JS webview.

use crate::ModuleTool;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

/// Closed widget kinds accepted by the host (fail-closed on unknown).
pub const WIDGET_KINDS: &[&str] = &[
    "column",
    "row",
    "heading",
    "text",
    "markdown",
    "stat_row",
    "table",
    "line_chart",
    "bar_chart",
    "pie",
    "scatter",
    "form",
    "button",
    "select",
    "radio",
    "checkbox",
    "textarea",
    "image",
    "audio",
    "empty_state",
    "count_label",
];

/// Modules shipped with the standard Preview profile (user may uninstall; choice persists).
pub const PREINSTALLED_MODULES: &[&str] = &["notes", "tasks", "ext-rt", "canvas"];

/// Host policy: modules the host refuses to uninstall (never granted by the package manifest).
pub const PROTECTED_BY_HOST_MODULES: &[&str] = &[];

/// Modules that keep dedicated hardcoded egui tabs in Preview (distinct from declarative sidebar).
pub const NATIVE_UI_MODULES: &[&str] = PREINSTALLED_MODULES;

/// Sidebar hides native-tab modules without a declarative surface yet.
/// Tasks uses generic navigation from lot 2 (#149).
pub const DECL_UI_SIDEBAR_EXCLUDE: &[&str] = &["notes", "ext-rt", "canvas"];

/// Deprecated alias — prefer [`is_preinstalled_module`] / [`is_protected_by_host`].
pub const BUNDLED_MODULES: &[&str] = PREINSTALLED_MODULES;

pub fn is_preinstalled_module(name: &str) -> bool {
    PREINSTALLED_MODULES.contains(&name)
}

pub fn is_protected_by_host(name: &str) -> bool {
    PROTECTED_BY_HOST_MODULES.contains(&name)
}

pub fn has_native_ui(name: &str) -> bool {
    NATIVE_UI_MODULES.contains(&name)
}

/// Deprecated — preinstalled ≠ protected; use [`is_protected_by_host`] for uninstall gates.
pub fn is_bundled_module(name: &str) -> bool {
    is_preinstalled_module(name)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeclUiError {
    BadType(String),
    MissingField(&'static str),
    UnknownKind(String),
    EmptyTitle,
}

impl fmt::Display for DeclUiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadType(t) => write!(f, "type must be declarative_ui, got {t}"),
            Self::MissingField(k) => write!(f, "missing field: {k}"),
            Self::UnknownKind(k) => write!(f, "unknown widget kind: {k}"),
            Self::EmptyTitle => write!(f, "title must not be empty"),
        }
    }
}

/// Package-local copy keyed by locale (`en`, `fr`, …) with an explicit fallback.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct DeclUiLabels {
    /// Locale code used when the requested language has no entry (e.g. `"en"`).
    pub fallback: String,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub en: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub fr: HashMap<String, String>,
}

/// Per-row action on a `table` widget (E15 lot 2 / #149).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeclUiRowAction {
    pub tool: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_key: Option<String>,
    pub args: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<DeclUiRowWhen>,
    /// Bind tools to re-fetch after a successful invoke (e.g. list after create).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_binds: Option<Vec<String>>,
}

/// Optional row filter for a [`DeclUiRowAction`].
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeclUiRowWhen {
    pub field: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eq: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ne: Option<serde_json::Value>,
}

/// Root document stored in `ui/index.html`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeclUiDocument {
    #[serde(rename = "type")]
    pub doc_type: String,
    pub title: String,
    /// Key into [`DeclUiLabels`] for the panel chrome heading (instead of raw [`Self::title`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poll_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels: Option<DeclUiLabels>,
    pub root: DeclUiWidget,
}

/// One node in the widget tree.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeclUiWidget {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Tool name whose JSON result feeds this widget (`table`, `stat_row`, `line_chart`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bind: Option<String>,
    /// JSON pointer or dotted path into the bind result (optional).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Tool invoked by `button` / submitted by `form`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub columns: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<DeclUiWidget>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<serde_json::Value>,
    /// Key into [`DeclUiDocument::labels`] for localized copy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_key: Option<String>,
    /// Label shown before inline form fields (e.g. « Nouvelle » / « New »).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix_label_key: Option<String>,
    /// When true, table renders rows without a header band.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hide_headers: Option<bool>,
    /// Per-row actions rendered beside each table row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_actions: Option<Vec<DeclUiRowAction>>,
    /// Bind tools to re-fetch after a successful invoke from this widget.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_binds: Option<Vec<String>>,
}

/// `module.ui` response — validated document ready for the egui host.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ModuleUiResponse {
    pub module: String,
    pub document: DeclUiDocument,
    /// Manifest tools (input schemas for forms). Omitted from the dumped JSON Schema.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(skip)]
    pub tools: Vec<ModuleTool>,
}

impl DeclUiDocument {
    /// Parse JSON bytes and validate the closed vocabulary.
    pub fn parse_json(raw: &[u8]) -> Result<Self, DeclUiError> {
        let doc: Self =
            serde_json::from_slice(raw).map_err(|e| DeclUiError::BadType(e.to_string()))?;
        doc.validate()?;
        Ok(doc)
    }

    pub fn validate(&self) -> Result<(), DeclUiError> {
        if self.doc_type != "declarative_ui" {
            return Err(DeclUiError::BadType(self.doc_type.clone()));
        }
        if self.title.trim().is_empty() {
            return Err(DeclUiError::EmptyTitle);
        }
        if let Some(labels) = &self.labels {
            labels.validate()?;
        }
        self.root.validate()?;
        Ok(())
    }

    /// Collect distinct tool names referenced by `bind` only (initial data load).
    pub fn bind_tools(&self) -> Vec<String> {
        let mut out = Vec::new();
        self.root.collect_binds(&mut out);
        out.sort();
        out.dedup();
        out
    }

    /// All tools referenced by `bind` or action widgets (audit / introspection).
    pub fn referenced_tools(&self) -> Vec<String> {
        let mut out = Vec::new();
        self.root.collect_tools(&mut out);
        out.sort();
        out.dedup();
        out
    }

    /// Flat list of widget kinds in the tree (gate / authoring checks).
    pub fn collect_widget_kinds(&self) -> Vec<String> {
        let mut out = Vec::new();
        self.root.collect_kinds(&mut out);
        out
    }

    /// Localized chrome title when [`Self::title_key`] and labels are present.
    pub fn chrome_title(&self, language: &str) -> String {
        if let (Some(key), Some(labels)) = (&self.title_key, &self.labels) {
            if let Some(text) = labels.resolve(language, key) {
                return text;
            }
        }
        self.title.clone()
    }

    /// Sidebar/catalogue title using the document fallback locale.
    pub fn catalogue_title(&self) -> String {
        if let (Some(key), Some(labels)) = (&self.title_key, &self.labels) {
            if let Some(text) = labels.resolve(&labels.fallback, key) {
                return text;
            }
        }
        self.title.clone()
    }
}

impl DeclUiLabels {
    pub fn validate(&self) -> Result<(), DeclUiError> {
        if self.fallback.trim().is_empty() {
            return Err(DeclUiError::MissingField("labels.fallback"));
        }
        Ok(())
    }

    /// Resolve a label key for `language` (`en`, `fr`, …) with explicit fallback.
    pub fn resolve(&self, language: &str, key: &str) -> Option<String> {
        locale_map(self, language)
            .and_then(|m| m.get(key).cloned())
            .or_else(|| locale_map(self, &self.fallback).and_then(|m| m.get(key).cloned()))
    }
}

fn locale_map<'a>(labels: &'a DeclUiLabels, language: &str) -> Option<&'a HashMap<String, String>> {
    match language.trim().to_ascii_lowercase().as_str() {
        "en" | "english" => Some(&labels.en),
        "fr" | "french" | "français" | "francais" => Some(&labels.fr),
        other if other == labels.fallback.trim().to_ascii_lowercase() => {
            locale_map_by_code(labels, other)
        }
        _ => None,
    }
}

fn locale_map_by_code<'a>(
    labels: &'a DeclUiLabels,
    code: &str,
) -> Option<&'a HashMap<String, String>> {
    match code {
        "en" => Some(&labels.en),
        "fr" => Some(&labels.fr),
        _ => None,
    }
}

impl DeclUiRowAction {
    pub fn validate(&self) -> Result<(), DeclUiError> {
        if self.tool.trim().is_empty() {
            return Err(DeclUiError::MissingField("row_action.tool"));
        }
        if self.label.as_ref().is_none_or(|l| l.is_empty())
            && self.label_key.as_ref().is_none_or(|k| k.is_empty())
        {
            return Err(DeclUiError::MissingField("row_action.label"));
        }
        Ok(())
    }
}

impl DeclUiRowWhen {
    pub fn matches(&self, row: &serde_json::Map<String, serde_json::Value>) -> bool {
        let val = row.get(&self.field);
        if let Some(expected) = &self.eq {
            return val == Some(expected);
        }
        if let Some(expected) = &self.ne {
            return val != Some(expected);
        }
        val.is_some()
    }
}

/// Substitute `$field` / `$row.field` placeholders in action args from a table row.
pub fn resolve_row_args(template: &serde_json::Value, row: &serde_json::Value) -> serde_json::Value {
    match template {
        serde_json::Value::String(s) => {
            if let Some(field) = s.strip_prefix('$') {
                let key = field.strip_prefix("row.").unwrap_or(field);
                return row
                    .get(key)
                    .cloned()
                    .unwrap_or_else(|| serde_json::Value::String(s.clone()));
            }
            serde_json::Value::String(s.clone())
        }
        serde_json::Value::Array(arr) => serde_json::Value::Array(
            arr.iter()
                .map(|v| resolve_row_args(v, row))
                .collect(),
        ),
        serde_json::Value::Object(map) => {
            let out: serde_json::Map<String, serde_json::Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), resolve_row_args(v, row)))
                .collect();
            serde_json::Value::Object(out)
        }
        other => other.clone(),
    }
}

impl DeclUiWidget {
    pub fn validate(&self) -> Result<(), DeclUiError> {
        if !WIDGET_KINDS.contains(&self.kind.as_str()) {
            return Err(DeclUiError::UnknownKind(self.kind.clone()));
        }
        match self.kind.as_str() {
            "column" | "row" => {
                let children = self
                    .children
                    .as_ref()
                    .ok_or(DeclUiError::MissingField("children"))?;
                for c in children {
                    c.validate()?;
                }
            }
            "heading" | "text" | "markdown" => {
                let has_text = self.text.as_ref().is_some_and(|t| !t.is_empty());
                let has_label_key = self.label_key.as_ref().is_some_and(|k| !k.is_empty());
                if !has_text && !has_label_key {
                    return Err(DeclUiError::MissingField("text"));
                }
            }
            "empty_state" => {
                if self.bind.as_ref().is_none_or(|b| b.is_empty()) {
                    return Err(DeclUiError::MissingField("bind"));
                }
                let has_text = self.text.as_ref().is_some_and(|t| !t.is_empty());
                let has_label_key = self.label_key.as_ref().is_some_and(|k| !k.is_empty());
                if !has_text && !has_label_key {
                    return Err(DeclUiError::MissingField("text"));
                }
            }
            "count_label" => {
                if self.bind.as_ref().is_none_or(|b| b.is_empty()) {
                    return Err(DeclUiError::MissingField("bind"));
                }
                if self.label_key.as_ref().is_none_or(|k| k.is_empty()) {
                    return Err(DeclUiError::MissingField("label_key"));
                }
            }
            "stat_row" | "table" | "line_chart" | "bar_chart" | "pie" | "scatter" => {
                if self.bind.as_ref().is_none_or(|b| b.is_empty()) {
                    return Err(DeclUiError::MissingField("bind"));
                }
                if self.kind == "table" {
                    if let Some(actions) = &self.row_actions {
                        for action in actions {
                            action.validate()?;
                        }
                    }
                }
            }
            "form" | "button" => {
                if self.tool.as_ref().is_none_or(|t| t.is_empty()) {
                    return Err(DeclUiError::MissingField("tool"));
                }
            }
            "select" | "radio" => {
                let has_items = self.items.as_ref().is_some_and(|i| !i.is_empty());
                let has_bind = self.bind.as_ref().is_some_and(|b| !b.is_empty());
                if !has_items && !has_bind {
                    return Err(DeclUiError::MissingField("items"));
                }
            }
            "checkbox" | "textarea" => {}
            "image" | "audio" => {
                let has_bind = self.bind.as_ref().is_some_and(|b| !b.is_empty());
                let has_text = self.text.as_ref().is_some_and(|t| !t.is_empty());
                if !has_bind && !has_text {
                    return Err(DeclUiError::MissingField("bind"));
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn collect_tools(&self, out: &mut Vec<String>) {
        if let Some(b) = &self.bind {
            if !b.is_empty() {
                out.push(b.clone());
            }
        }
        if let Some(t) = &self.tool {
            if !t.is_empty() {
                out.push(t.clone());
            }
        }
        if let Some(children) = &self.children {
            for c in children {
                c.collect_tools(out);
            }
        }
        if let Some(actions) = &self.row_actions {
            for action in actions {
                if !action.tool.is_empty() {
                    out.push(action.tool.clone());
                }
            }
        }
    }

    fn collect_binds(&self, out: &mut Vec<String>) {
        if let Some(b) = &self.bind {
            if !b.is_empty() {
                out.push(b.clone());
            }
        }
        if let Some(children) = &self.children {
            for c in children {
                c.collect_binds(out);
            }
        }
    }

    fn collect_kinds(&self, out: &mut Vec<String>) {
        out.push(self.kind.clone());
        if let Some(children) = &self.children {
            for c in children {
                c.collect_kinds(out);
            }
        }
    }
}

/// Whether a module should appear as a dynamic declarative tab in the sidebar.
pub fn sidebar_decl_ui_module(name: &str, ui_mode: Option<&str>) -> bool {
    ui_mode == Some("declarative_ui") && !DECL_UI_SIDEBAR_EXCLUDE.contains(&name)
}

/// Build a minimal default UI tree for scaffold/package (P07.4).
pub fn default_document(
    title: &str,
    primary_tool: &str,
    input_schema: &serde_json::Value,
) -> DeclUiDocument {
    fn blank(kind: &str) -> DeclUiWidget {
        DeclUiWidget {
            kind: kind.into(),
            text: None,
            label: None,
            bind: None,
            source: None,
            tool: None,
            columns: None,
            items: None,
            series: None,
            children: None,
            args: None,
            label_key: None,
            prefix_label_key: None,
            hide_headers: None,
            row_actions: None,
            refresh_binds: None,
        }
    }
    let mut children = vec![
        {
            let mut w = blank("heading");
            w.text = Some(title.to_string());
            w
        },
        {
            let mut w = blank("form");
            w.label = Some("Run".into());
            w.tool = Some(primary_tool.to_string());
            w.args = Some(input_schema.clone());
            w
        },
        {
            let mut w = blank("table");
            w.bind = Some(primary_tool.to_string());
            w
        },
    ];
    if primary_tool.ends_with(".snapshot") || primary_tool.contains("list") {
        children.retain(|w| w.kind != "form");
        let mut w = blank("button");
        w.label = Some("Refresh".into());
        w.tool = Some(primary_tool.to_string());
        w.args = Some(serde_json::json!({}));
        children.push(w);
    }
    if let Some((key, values)) = first_enum_property(input_schema) {
        let mut w = blank("select");
        w.label = Some(key);
        w.tool = Some(primary_tool.to_string());
        w.items = Some(values);
        children.insert(1, w);
    }
    DeclUiDocument {
        doc_type: "declarative_ui".into(),
        title: title.to_string(),
        poll_ms: None,
        labels: None,
        title_key: None,
        root: {
            let mut w = blank("column");
            w.children = Some(children);
            w
        },
    }
}

pub fn document_to_json(doc: &DeclUiDocument) -> String {
    serde_json::to_string_pretty(doc).expect("decl ui json")
}

fn first_enum_property(schema: &serde_json::Value) -> Option<(String, Vec<String>)> {
    let props = schema.get("properties")?.as_object()?;
    for (k, v) in props {
        if let Some(arr) = v.get("enum").and_then(|e| e.as_array()) {
            let vals: Vec<String> = arr
                .iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect();
            if !vals.is_empty() {
                return Some((k.clone(), vals));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preinstalled_is_not_protected_by_default() {
        assert!(is_preinstalled_module("tasks"));
        assert!(!is_protected_by_host("tasks"));
        assert!(has_native_ui("tasks"));
    }

    #[test]
    fn accepts_pie_and_scatter() {
        let raw = br#"{
            "type":"declarative_ui",
            "title":"Charts",
            "root":{"kind":"column","children":[
                {"kind":"pie","bind":"stats.breakdown"},
                {"kind":"scatter","bind":"stats.points","series":"y"}
            ]}
        }"#;
        DeclUiDocument::parse_json(raw).expect("pie+scatter valid");
    }

    #[test]
    fn rejects_webview_kind() {
        let raw = br#"{"type":"declarative_ui","title":"X","root":{"kind":"webview"}}"#;
        let err = DeclUiDocument::parse_json(raw).unwrap_err();
        assert!(matches!(err, DeclUiError::UnknownKind(_)));
    }

    #[test]
    fn accepts_minimal_column() {
        let raw = br#"{
            "type":"declarative_ui",
            "title":"Demo",
            "root":{"kind":"column","children":[
                {"kind":"heading","text":"Hi"},
                {"kind":"button","tool":"demo.run","label":"Go"}
            ]}
        }"#;
        DeclUiDocument::parse_json(raw).expect("valid");
    }

    #[test]
    fn default_document_validates() {
        let doc = default_document("netmon", "netmon.snapshot", &serde_json::json!({}));
        doc.validate().expect("default ok");
    }

    #[test]
    fn default_document_select_when_enum() {
        let schema = serde_json::json!({
            "type":"object",
            "properties":{"mode":{"type":"string","enum":["a","b"]}}
        });
        let doc = default_document("demo", "demo.run", &schema);
        doc.validate().expect("ok");
        assert!(doc.collect_widget_kinds().iter().any(|k| k == "select"));
    }

    #[test]
    fn accepts_bar_chart_and_checkbox() {
        let raw = br#"{
            "type":"declarative_ui",
            "title":"Demo",
            "root":{"kind":"column","children":[
                {"kind":"checkbox","label":"On"},
                {"kind":"bar_chart","bind":"demo.stats"},
                {"kind":"select","items":["x","y"]}
            ]}
        }"#;
        DeclUiDocument::parse_json(raw).expect("valid");
    }

    #[test]
    fn tasks_uses_generic_sidebar_when_declarative() {
        assert!(sidebar_decl_ui_module("tasks", Some("declarative_ui")));
        assert!(!sidebar_decl_ui_module("notes", Some("declarative_ui")));
    }

    #[test]
    fn labels_resolve_with_explicit_fallback() {
        let labels = DeclUiLabels {
            fallback: "en".into(),
            en: HashMap::from([("create".into(), "Create".into())]),
            fr: HashMap::from([("create".into(), "Créer".into())]),
        };
        assert_eq!(labels.resolve("fr", "create").as_deref(), Some("Créer"));
        assert_eq!(labels.resolve("de", "create").as_deref(), Some("Create"));
    }

    #[test]
    fn resolve_row_args_substitutes_fields() {
        let row = serde_json::json!({"id":"task-1","done":false});
        let args = resolve_row_args(
            &serde_json::json!({"id":"$id","done":true}),
            &row,
        );
        assert_eq!(args["id"], "task-1");
        assert_eq!(args["done"], true);
    }

    #[test]
    fn accepts_tasks_declarative_document() {
        let raw = include_bytes!("../../../share/modules/tasks.aospkg/ui/index.html");
        let doc = DeclUiDocument::parse_json(raw).expect("tasks ui valid");
        let labels = doc.labels.as_ref().expect("tasks labels");
        assert_eq!(
            labels.resolve("fr", "tasks_new").as_deref(),
            Some("Nouvelle")
        );
        assert_eq!(
            labels.resolve("en", "tasks_empty").as_deref(),
            Some("No tasks — create one or ask an agent `tasks.create`.")
        );
        assert_eq!(doc.chrome_title("fr"), "Tâches");
    }
}
