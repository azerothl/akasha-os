//! Host-rendered declarative module UI (E15 / Preview 0.7).
//!
//! Modules ship a JSON widget tree in `ui/index.html` (`type: declarative_ui`).
//! The egui host paints a closed vocabulary — no HTML/JS webview.

use crate::ModuleTool;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
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
    "form",
    "button",
    "select",
    "radio",
    "checkbox",
    "textarea",
    "image",
    "audio",
];

/// Bundled modules that must not be uninstalled (boot would restore them).
pub const BUNDLED_MODULES: &[&str] = &["notes", "tasks", "ext-rt"];

/// Modules that keep dedicated hardcoded egui tabs in Preview.
pub const DECL_UI_SIDEBAR_EXCLUDE: &[&str] = BUNDLED_MODULES;

pub fn is_bundled_module(name: &str) -> bool {
    BUNDLED_MODULES.contains(&name)
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

/// Root document stored in `ui/index.html`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeclUiDocument {
    #[serde(rename = "type")]
    pub doc_type: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poll_ms: Option<u64>,
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
                if self.text.as_ref().is_none_or(|t| t.is_empty()) {
                    return Err(DeclUiError::MissingField("text"));
                }
            }
            "stat_row" | "table" | "line_chart" | "bar_chart" => {
                if self.bind.as_ref().is_none_or(|b| b.is_empty()) {
                    return Err(DeclUiError::MissingField("bind"));
                }
            }
            "form" | "button" => {
                if self.tool.as_ref().is_none_or(|t| t.is_empty()) {
                    return Err(DeclUiError::MissingField("tool"));
                }
            }
            "select" | "radio" => {
                let has_items = self
                    .items
                    .as_ref()
                    .is_some_and(|i| !i.is_empty());
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
pub fn default_document(title: &str, primary_tool: &str, input_schema: &serde_json::Value) -> DeclUiDocument {
    let mut children = vec![
        DeclUiWidget {
            kind: "heading".into(),
            text: Some(title.to_string()),
            label: None,
            bind: None,
            source: None,
            tool: None,
            columns: None,
            items: None,
            series: None,
            children: None,
            args: None,
        },
        DeclUiWidget {
            kind: "form".into(),
            text: None,
            label: Some("Run".into()),
            bind: None,
            source: None,
            tool: Some(primary_tool.to_string()),
            columns: None,
            items: None,
            series: None,
            children: None,
            args: Some(input_schema.clone()),
        },
        DeclUiWidget {
            kind: "table".into(),
            text: None,
            label: None,
            bind: Some(primary_tool.to_string()),
            source: None,
            tool: None,
            columns: None,
            items: None,
            series: None,
            children: None,
            args: None,
        },
    ];
    if primary_tool.ends_with(".snapshot") || primary_tool.contains("list") {
        children.retain(|w| w.kind != "form");
        children.push(DeclUiWidget {
            kind: "button".into(),
            text: None,
            label: Some("Refresh".into()),
            bind: None,
            source: None,
            tool: Some(primary_tool.to_string()),
            columns: None,
            items: None,
            series: None,
            children: None,
            args: Some(serde_json::json!({})),
        });
    }
    if let Some((key, values)) = first_enum_property(input_schema) {
        children.insert(
            1,
            DeclUiWidget {
                kind: "select".into(),
                text: None,
                label: Some(key),
                bind: None,
                source: None,
                tool: Some(primary_tool.to_string()),
                columns: None,
                items: Some(values),
                series: None,
                children: None,
                args: None,
            },
        );
    }
    DeclUiDocument {
        doc_type: "declarative_ui".into(),
        title: title.to_string(),
        poll_ms: None,
        root: DeclUiWidget {
            kind: "column".into(),
            text: None,
            label: None,
            bind: None,
            source: None,
            tool: None,
            columns: None,
            items: None,
            series: None,
            children: Some(children),
            args: None,
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
    fn rejects_unknown_kind() {
        let raw = br#"{"type":"declarative_ui","title":"X","root":{"kind":"canvas"}}"#;
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
}
