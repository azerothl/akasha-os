//! Rich declarative UI contract v2 (issue #150 lot 1).
//!
//! State slots, bindings, actions, predicates, subscriptions, and v2 widget
//! validation. See `docs/rich-app-contract.md`.

use crate::decl_ui::{DeclUiDocument, DeclUiError, DeclUiWidget};
use crate::rich_app_contract::{
    HOST_UI_CONTRACT_MAX, INTERACTION_PHASES, JOBS_SERVICE_VERSION, JOB_STATES,
    MAX_BINDINGS, MAX_PREDICATE_DEPTH, MAX_PREDICATE_NODES, MAX_STATE_SLOTS,
    MAX_STATE_STRING_LENGTH, MAX_SUBSCRIPTIONS, MAX_UI_DEPTH, MAX_UI_NODES,
    MEDIA_GENERATE_CAP, MEDIA_IMAGE_SERVICE_VERSION, PLATFORM_MEDIA_IMAGE_METHODS,
    UI_CONTRACT_V2, UI_V2_ADDITIONAL_WIDGET_KINDS,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// All widget kinds accepted for a given UI contract major version.
pub fn widget_kinds_for_contract(contract: u32) -> Vec<&'static str> {
    let mut kinds = crate::decl_ui::WIDGET_KINDS.to_vec();
    if contract >= UI_CONTRACT_V2 {
        kinds.extend_from_slice(UI_V2_ADDITIONAL_WIDGET_KINDS);
    }
    kinds
}

/// Whether `kind` is valid for `contract`.
pub fn is_widget_kind_valid(kind: &str, contract: u32) -> bool {
    widget_kinds_for_contract(contract).contains(&kind)
}

/// Declared state slot schema (local or document).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct RichStateSlot {
    #[serde(rename = "type")]
    pub slot_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nullable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct RichStateDecl {
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub local: HashMap<String, RichStateSlot>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub document: HashMap<String, RichStateSlot>,
}

/// Tool binding — fetches module tool JSON into renderer state.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct RichBinding {
    pub id: String,
    pub tool: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// Named state/resource keys that invalidate this binding (targeted refresh).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invalidate_on: Vec<String>,
}

/// Declarative action — module tool or platform service.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct RichAction {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refresh_binds: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invalidate_on: Vec<String>,
}

/// Job/event subscription scoped to one app instance.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct RichSubscription {
    pub id: String,
    /// Action id whose returned `job_id` is tracked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

/// Semantic interaction event payload (host → module reducer).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct RichInteractionEvent {
    pub phase: String,
    pub interaction_id: String,
    #[serde(default)]
    pub value: Value,
}

/// Generic job handle exposed to packages.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
pub struct RichJobHandle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<RichJobProgress>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
pub struct RichJobProgress {
    #[serde(default)]
    pub completed: u32,
    #[serde(default)]
    pub total: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RichDeclUiError {
    ContractUnsupported(u32),
    ServicesJobsTooNew { required: u32, host: u32 },
    ServicesMediaImageTooNew { required: u32, host: u32 },
    MissingServicesJobs,
    MissingServicesMediaImage,
    TooManyNodes { count: u32, max: u32 },
    TooDeep { depth: u32, max: u32 },
    TooManyStateSlots { count: u32, max: u32 },
    TooManyBindings { count: u32, max: u32 },
    TooManySubscriptions { count: u32, max: u32 },
    UnknownTool(String),
    UnknownAction(String),
    UnknownService(String),
    MissingCapability(String),
    InvalidSubstitution(String),
    InvalidPredicate(String),
    Widget(DeclUiError),
}

impl std::fmt::Display for RichDeclUiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ContractUnsupported(v) => write!(f, "ui.contract {v} not supported (host max {HOST_UI_CONTRACT_MAX})"),
            Self::ServicesJobsTooNew { required, host } => {
                write!(f, "services.jobs {required} > host {host}")
            }
            Self::ServicesMediaImageTooNew { required, host } => {
                write!(f, "services.media_image {required} > host {host}")
            }
            Self::MissingServicesJobs => write!(f, "manifest requires services.jobs for job widgets/actions"),
            Self::MissingServicesMediaImage => {
                write!(f, "manifest requires services.media_image for media image service actions")
            }
            Self::TooManyNodes { count, max } => write!(f, "too many widget nodes: {count} > {max}"),
            Self::TooDeep { depth, max } => write!(f, "widget tree too deep: {depth} > {max}"),
            Self::TooManyStateSlots { count, max } => write!(f, "too many state slots: {count} > {max}"),
            Self::TooManyBindings { count, max } => write!(f, "too many bindings: {count} > {max}"),
            Self::TooManySubscriptions { count, max } => {
                write!(f, "too many subscriptions: {count} > {max}")
            }
            Self::UnknownTool(t) => write!(f, "unknown tool reference: {t}"),
            Self::UnknownAction(a) => write!(f, "unknown action reference: {a}"),
            Self::UnknownService(s) => write!(f, "unknown or disallowed service: {s}"),
            Self::MissingCapability(c) => write!(f, "action requires undeclared capability: {c}"),
            Self::InvalidSubstitution(s) => write!(f, "invalid substitution: {s}"),
            Self::InvalidPredicate(p) => write!(f, "invalid predicate: {p}"),
            Self::Widget(e) => write!(f, "{e}"),
        }
    }
}

impl From<DeclUiError> for RichDeclUiError {
    fn from(e: DeclUiError) -> Self {
        Self::Widget(e)
    }
}

impl RichStateDecl {
    pub fn slot_count(&self) -> u32 {
        (self.local.len() + self.document.len()) as u32
    }

    pub fn validate(&self) -> Result<(), RichDeclUiError> {
        for (name, slot) in self.local.iter().chain(self.document.iter()) {
            slot.validate(name)?;
        }
        Ok(())
    }
}

impl RichStateSlot {
    pub fn validate(&self, name: &str) -> Result<(), RichDeclUiError> {
        match self.slot_type.as_str() {
            "string" | "number" | "boolean" => {}
            other => {
                return Err(RichDeclUiError::InvalidSubstitution(format!(
                    "state.{name}: unknown type {other}"
                )));
            }
        }
        if let Some(max) = self.max_length {
            if max > MAX_STATE_STRING_LENGTH {
                return Err(RichDeclUiError::InvalidSubstitution(format!(
                    "state.{name}: max_length {max} > {MAX_STATE_STRING_LENGTH}"
                )));
            }
        }
        Ok(())
    }
}

impl RichBinding {
    pub fn validate(&self, tools: &HashSet<String>) -> Result<(), RichDeclUiError> {
        if self.id.trim().is_empty() {
            return Err(RichDeclUiError::Widget(DeclUiError::MissingField(
                "bindings.id",
            )));
        }
        if self.tool.trim().is_empty() {
            return Err(RichDeclUiError::Widget(DeclUiError::MissingField(
                "bindings.tool",
            )));
        }
        if !tools.contains(&self.tool) {
            return Err(RichDeclUiError::UnknownTool(self.tool.clone()));
        }
        Ok(())
    }
}

impl RichAction {
    pub fn validate(
        &self,
        tools: &HashSet<String>,
        granted_caps: &[String],
    ) -> Result<(), RichDeclUiError> {
        if self.id.trim().is_empty() {
            return Err(RichDeclUiError::Widget(DeclUiError::MissingField(
                "actions.id",
            )));
        }
        let has_tool = self.tool.as_ref().is_some_and(|t| !t.is_empty());
        let has_service = self.service.as_ref().is_some_and(|s| !s.is_empty());
        if has_tool == has_service {
            return Err(RichDeclUiError::InvalidSubstitution(format!(
                "action {}: exactly one of tool or service required",
                self.id
            )));
        }
        if let Some(tool) = &self.tool {
            if !tools.contains(tool) {
                return Err(RichDeclUiError::UnknownTool(tool.clone()));
            }
        }
        if let Some(service) = &self.service {
            validate_service_action(service, granted_caps)?;
            if let Some(input) = &self.input {
                validate_substitutions(input)?;
            }
        } else if let Some(input) = &self.input {
            validate_substitutions(input)?;
        }
        Ok(())
    }
}

impl RichSubscription {
    pub fn validate(&self, actions: &HashSet<String>) -> Result<(), RichDeclUiError> {
        if self.id.trim().is_empty() {
            return Err(RichDeclUiError::Widget(DeclUiError::MissingField(
                "subscriptions.id",
            )));
        }
        let has_action = self.action.as_ref().is_some_and(|a| !a.is_empty());
        let has_job = self.job_id.as_ref().is_some_and(|j| !j.is_empty());
        if !has_action && !has_job {
            return Err(RichDeclUiError::InvalidSubstitution(format!(
                "subscription {}: action or job_id required",
                self.id
            )));
        }
        if let Some(action) = &self.action {
            if !actions.contains(action) {
                return Err(RichDeclUiError::UnknownAction(action.clone()));
            }
        }
        Ok(())
    }
}

/// Validate manifest service version gates against the host.
pub fn validate_manifest_services(
    jobs: Option<u32>,
    media_image: Option<u32>,
    document: &DeclUiDocument,
    contract: u32,
) -> Result<(), RichDeclUiError> {
    if contract < UI_CONTRACT_V2 {
        return Ok(());
    }
    let needs_jobs = document.uses_jobs();
    let needs_media = document.uses_media_image_service();
    if needs_jobs {
        let required = jobs.ok_or(RichDeclUiError::MissingServicesJobs)?;
        if required > JOBS_SERVICE_VERSION {
            return Err(RichDeclUiError::ServicesJobsTooNew {
                required,
                host: JOBS_SERVICE_VERSION,
            });
        }
    }
    if needs_media {
        let required = media_image.ok_or(RichDeclUiError::MissingServicesMediaImage)?;
        if required > MEDIA_IMAGE_SERVICE_VERSION {
            return Err(RichDeclUiError::ServicesMediaImageTooNew {
                required,
                host: MEDIA_IMAGE_SERVICE_VERSION,
            });
        }
    }
    Ok(())
}

pub fn validate_ui_contract_supported(contract: u32) -> Result<(), RichDeclUiError> {
    if contract > HOST_UI_CONTRACT_MAX {
        return Err(RichDeclUiError::ContractUnsupported(contract));
    }
    Ok(())
}

/// Full v2 document validation (tree, state, bindings, actions, subscriptions).
pub fn validate_rich_document(
    doc: &DeclUiDocument,
    contract: u32,
    manifest_tools: &[&str],
    granted_caps: &[String],
) -> Result<(), RichDeclUiError> {
    validate_ui_contract_supported(contract)?;
    if contract < UI_CONTRACT_V2 {
        return Ok(());
    }
    let (nodes, depth) = count_nodes_and_depth(&doc.root, 1);
    if nodes > MAX_UI_NODES {
        return Err(RichDeclUiError::TooManyNodes {
            count: nodes,
            max: MAX_UI_NODES,
        });
    }
    if depth > MAX_UI_DEPTH {
        return Err(RichDeclUiError::TooDeep {
            depth,
            max: MAX_UI_DEPTH,
        });
    }
    if let Some(state) = &doc.state {
        if state.slot_count() > MAX_STATE_SLOTS {
            return Err(RichDeclUiError::TooManyStateSlots {
                count: state.slot_count(),
                max: MAX_STATE_SLOTS,
            });
        }
        state.validate()?;
    }
    if doc.bindings.len() as u32 > MAX_BINDINGS {
        return Err(RichDeclUiError::TooManyBindings {
            count: doc.bindings.len() as u32,
            max: MAX_BINDINGS,
        });
    }
    if doc.subscriptions.len() as u32 > MAX_SUBSCRIPTIONS {
        return Err(RichDeclUiError::TooManySubscriptions {
            count: doc.subscriptions.len() as u32,
            max: MAX_SUBSCRIPTIONS,
        });
    }
    let tools: HashSet<String> = manifest_tools.iter().map(|s| (*s).to_string()).collect();
    for binding in &doc.bindings {
        binding.validate(&tools)?;
    }
    for action in &doc.actions {
        action.validate(&tools, granted_caps)?;
    }
    let action_ids: HashSet<String> = doc.actions.iter().map(|a| a.id.clone()).collect();
    for sub in &doc.subscriptions {
        sub.validate(&action_ids)?;
    }
    validate_widget_tree(&doc.root, contract)?;
    Ok(())
}

fn count_nodes_and_depth(w: &DeclUiWidget, depth: u32) -> (u32, u32) {
    let mut nodes = 1u32;
    let mut max_depth = depth;
    if let Some(children) = &w.children {
        for c in children {
            let (n, d) = count_nodes_and_depth(c, depth + 1);
            nodes += n;
            max_depth = max_depth.max(d);
        }
    }
    if let Some(tabs) = &w.tabs {
        for tab in tabs {
            if let Some(child) = &tab.content {
                let (n, d) = count_nodes_and_depth(child, depth + 1);
                nodes += n;
                max_depth = max_depth.max(d);
            }
        }
    }
    (nodes, max_depth)
}

fn validate_widget_tree(w: &DeclUiWidget, contract: u32) -> Result<(), RichDeclUiError> {
    if !is_widget_kind_valid(&w.kind, contract) {
        return Err(RichDeclUiError::Widget(DeclUiError::UnknownKind(
            w.kind.clone(),
        )));
    }
    if let Some(pred) = &w.visible {
        validate_predicate(pred)?;
    }
    if let Some(pred) = &w.enabled {
        validate_predicate(pred)?;
    }
    match w.kind.as_str() {
        "column" | "row" | "scroll" | "section" => {
            let children = w
                .children
                .as_ref()
                .ok_or(RichDeclUiError::Widget(DeclUiError::MissingField(
                    "children",
                )))?;
            for c in children {
                validate_widget_tree(c, contract)?;
            }
        }
        "split" => {
            let children = w
                .children
                .as_ref()
                .ok_or(RichDeclUiError::Widget(DeclUiError::MissingField(
                    "children",
                )))?;
            if children.len() != 2 {
                return Err(RichDeclUiError::InvalidSubstitution(
                    "split requires exactly 2 children".into(),
                ));
            }
            for c in children {
                validate_widget_tree(c, contract)?;
            }
        }
        "tabs" => {
            let tabs = w
                .tabs
                .as_ref()
                .ok_or(RichDeclUiError::Widget(DeclUiError::MissingField("tabs")))?;
            if tabs.is_empty() {
                return Err(RichDeclUiError::Widget(DeclUiError::MissingField("tabs")));
            }
            for tab in tabs {
                if let Some(content) = &tab.content {
                    validate_widget_tree(content, contract)?;
                }
            }
        }
        "slider" | "number" => {
            if w.state_key.as_ref().is_none_or(|k| k.is_empty()) {
                return Err(RichDeclUiError::Widget(DeclUiError::MissingField("state_key")));
            }
        }
        "text_input" | "file_picker" => {
            if w.state_key.as_ref().is_none_or(|k| k.is_empty()) {
                return Err(RichDeclUiError::Widget(DeclUiError::MissingField("state_key")));
            }
        }
        "progress" => {
            let has_bind = w.bind.as_ref().is_some_and(|b| !b.is_empty());
            let has_binding = w.binding.as_ref().is_some_and(|b| !b.is_empty());
            let has_key = w.state_key.as_ref().is_some_and(|k| !k.is_empty());
            if !has_bind && !has_binding && !has_key {
                return Err(RichDeclUiError::Widget(DeclUiError::MissingField("bind")));
            }
        }
        "job" => {
            let has_sub = w.subscription.as_ref().is_some_and(|s| !s.is_empty());
            let has_action = w.action.as_ref().is_some_and(|a| !a.is_empty());
            if !has_sub && !has_action {
                return Err(RichDeclUiError::Widget(DeclUiError::MissingField(
                    "subscription",
                )));
            }
        }
        "image_view" => {
            let has_bind = w.bind.as_ref().is_some_and(|b| !b.is_empty());
            let has_binding = w.binding.as_ref().is_some_and(|b| !b.is_empty());
            let has_resource = w.resource.as_ref().is_some_and(|r| !r.is_empty());
            let has_text = w.text.as_ref().is_some_and(|t| !t.is_empty());
            if !has_bind && !has_binding && !has_resource && !has_text {
                return Err(RichDeclUiError::Widget(DeclUiError::MissingField("bind")));
            }
        }
        "layer_canvas" => {
            if w.layers_key.as_ref().is_none_or(|k| k.is_empty()) {
                return Err(RichDeclUiError::Widget(DeclUiError::MissingField("layers_key")));
            }
            if w.selected_key.as_ref().is_none_or(|k| k.is_empty()) {
                return Err(RichDeclUiError::Widget(DeclUiError::MissingField("selected_key")));
            }
        }
        "layer_list" => {
            if w.layers_key.as_ref().is_none_or(|k| k.is_empty()) {
                return Err(RichDeclUiError::Widget(DeclUiError::MissingField("layers_key")));
            }
            if w.selected_key.as_ref().is_none_or(|k| k.is_empty()) {
                return Err(RichDeclUiError::Widget(DeclUiError::MissingField("selected_key")));
            }
        }
        "undo_redo" => {
            if w.canvas_id.as_ref().is_none_or(|k| k.is_empty()) {
                return Err(RichDeclUiError::Widget(DeclUiError::MissingField("canvas_id")));
            }
            if w.layers_key.as_ref().is_none_or(|k| k.is_empty()) {
                return Err(RichDeclUiError::Widget(DeclUiError::MissingField("layers_key")));
            }
        }
        "spacer" => {}
        other => {
            // Delegate v1 widget rules.
            w.validate_v1_only(other, contract)?;
        }
    }
    Ok(())
}

fn validate_service_action(service: &str, granted_caps: &[String]) -> Result<(), RichDeclUiError> {
    match service {
        "jobs.demo.start" | "jobs.demo.cancel" | "job.cancel" => Ok(()),
        "media.image.generate" | "media.image.cancel" | "media.image.upscale" => {
            if !PLATFORM_MEDIA_IMAGE_METHODS.contains(&service) {
                return Err(RichDeclUiError::UnknownService(service.into()));
            }
            if !granted_caps.iter().any(|c| c == MEDIA_GENERATE_CAP) {
                return Err(RichDeclUiError::MissingCapability(
                    MEDIA_GENERATE_CAP.into(),
                ));
            }
            Ok(())
        }
        "files.save_as" => {
            if !granted_caps.iter().any(|c| c.starts_with("fs.read:/downloads/")) {
                return Err(RichDeclUiError::MissingCapability(
                    "fs.read:/downloads/**".into(),
                ));
            }
            Ok(())
        }
        other => Err(RichDeclUiError::UnknownService(other.into())),
    }
}

/// Resolve `$local.*`, `$document.*`, `$row.*` placeholders in action input JSON.
pub fn resolve_action_input(template: &Value, local: &HashMap<String, Value>, document: &HashMap<String, Value>) -> Value {
    resolve_action_input_row(template, local, document, None)
}

pub fn resolve_action_input_row(
    template: &Value,
    local: &HashMap<String, Value>,
    document: &HashMap<String, Value>,
    row: Option<&Value>,
) -> Value {
    match template {
        Value::String(s) => {
            if let Some(rest) = s.strip_prefix('$') {
                if let Some(key) = rest.strip_prefix("local.") {
                    return local
                        .get(key)
                        .cloned()
                        .unwrap_or(Value::String(s.clone()));
                }
                if let Some(key) = rest.strip_prefix("document.") {
                    return document
                        .get(key)
                        .cloned()
                        .unwrap_or(Value::String(s.clone()));
                }
                if let (Some(r), Some(key)) = (row, rest.strip_prefix("row.")) {
                    return r
                        .get(key)
                        .cloned()
                        .unwrap_or(Value::String(s.clone()));
                }
                if let (Some(r), None) = (row, Some(rest)) {
                    if !rest.contains('.') {
                        return r
                            .get(rest)
                            .cloned()
                            .unwrap_or(Value::String(s.clone()));
                    }
                }
            }
            Value::String(s.clone())
        }
        Value::Array(arr) => Value::Array(
            arr.iter()
                .map(|v| resolve_action_input_row(v, local, document, row))
                .collect(),
        ),
        Value::Object(map) => {
            let out: serde_json::Map<String, Value> = map
                .iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        resolve_action_input_row(v, local, document, row),
                    )
                })
                .collect();
            Value::Object(out)
        }
        other => other.clone(),
    }
}

pub fn validate_substitutions(value: &Value) -> Result<(), RichDeclUiError> {
    match value {
        Value::String(s) if s.starts_with('$') => {
            let rest = s.trim_start_matches('$');
            if !(rest.starts_with("local.")
                || rest.starts_with("document.")
                || rest.starts_with("row.")
                || (!rest.contains('.') && rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')))
            {
                return Err(RichDeclUiError::InvalidSubstitution(s.clone()));
            }
            Ok(())
        }
        Value::Array(arr) => {
            for v in arr {
                validate_substitutions(v)?;
            }
            Ok(())
        }
        Value::Object(map) => {
            for v in map.values() {
                validate_substitutions(v)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Minimal typed predicate AST (JSON-encoded).
pub fn validate_predicate(value: &Value) -> Result<(), RichDeclUiError> {
    validate_predicate_depth(value, 0, &mut 0)
}

fn validate_predicate_depth(
    value: &Value,
    depth: u32,
    nodes: &mut u32,
) -> Result<(), RichDeclUiError> {
    *nodes += 1;
    if *nodes > MAX_PREDICATE_NODES {
        return Err(RichDeclUiError::InvalidPredicate(format!(
            "predicate exceeds {MAX_PREDICATE_NODES} nodes"
        )));
    }
    if depth > MAX_PREDICATE_DEPTH {
        return Err(RichDeclUiError::InvalidPredicate(format!(
            "predicate exceeds depth {MAX_PREDICATE_DEPTH}"
        )));
    }
    match value {
        Value::Bool(_) => Ok(()),
        Value::Object(map) => {
            if map.len() != 1 {
                return Err(RichDeclUiError::InvalidPredicate(
                    "predicate object must have one operator key".into(),
                ));
            }
            for (op, inner) in map {
                match op.as_str() {
                    "and" | "or" => {
                        let arr = inner.as_array().ok_or_else(|| {
                            RichDeclUiError::InvalidPredicate(format!("{op} expects array"))
                        })?;
                        for v in arr {
                            validate_predicate_depth(v, depth + 1, nodes)?;
                        }
                    }
                    "not" => validate_predicate_depth(inner, depth + 1, nodes)?,
                    "eq" | "ne" | "gt" | "lt" => {
                        let arr = inner.as_array().ok_or_else(|| {
                            RichDeclUiError::InvalidPredicate(format!("{op} expects array"))
                        })?;
                        if arr.len() != 2 {
                            return Err(RichDeclUiError::InvalidPredicate(format!(
                                "{op} expects 2 operands"
                            )));
                        }
                        for v in arr {
                            validate_predicate_ref(v)?;
                        }
                    }
                    "ref" => validate_predicate_ref(inner)?,
                    other => {
                        return Err(RichDeclUiError::InvalidPredicate(format!(
                            "unknown predicate operator: {other}"
                        )));
                    }
                }
            }
            Ok(())
        }
        _ => Err(RichDeclUiError::InvalidPredicate(
            "predicate must be bool or operator object".into(),
        )),
    }
}

fn validate_predicate_ref(value: &Value) -> Result<(), RichDeclUiError> {
    match value {
        Value::Bool(_) | Value::Number(_) | Value::String(_) => {
            if let Value::String(s) = value {
                if s.starts_with('$') {
                    validate_substitutions(value)?;
                }
            }
            Ok(())
        }
        _ => Err(RichDeclUiError::InvalidPredicate(
            "predicate operand must be literal or $ref".into(),
        )),
    }
}

/// Evaluate predicate against local/document state (host-side enablement/visibility).
pub fn eval_predicate(
    value: &Value,
    local: &HashMap<String, Value>,
    document: &HashMap<String, Value>,
) -> bool {
    match value {
        Value::Bool(b) => *b,
        Value::Object(map) => {
            if let Some((op, inner)) = map.iter().next() {
                match op.as_str() {
                    "and" => inner
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .all(|v| eval_predicate(v, local, document))
                        })
                        .unwrap_or(false),
                    "or" => inner
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .any(|v| eval_predicate(v, local, document))
                        })
                        .unwrap_or(false),
                    "not" => !eval_predicate(inner, local, document),
                    "eq" => eval_binary(inner, local, document, |a, b| a == b),
                    "ne" => eval_binary(inner, local, document, |a, b| a != b),
                    "gt" => eval_binary(inner, local, document, |a, b| cmp_f64(a, b) == Some(std::cmp::Ordering::Greater)),
                    "lt" => eval_binary(inner, local, document, |a, b| cmp_f64(a, b) == Some(std::cmp::Ordering::Less)),
                    "ref" => eval_ref(inner, local, document).map(|v| truthy(&v)).unwrap_or(false),
                    _ => false,
                }
            } else {
                false
            }
        }
        _ => false,
    }
}

fn eval_binary<F>(inner: &Value, local: &HashMap<String, Value>, document: &HashMap<String, Value>, f: F) -> bool
where
    F: Fn(&Value, &Value) -> bool,
{
    let arr = match inner.as_array() {
        Some(a) if a.len() == 2 => a,
        _ => return false,
    };
    match (eval_ref(&arr[0], local, document), eval_ref(&arr[1], local, document)) {
        (Some(a), Some(b)) => f(&a, &b),
        _ => false,
    }
}

fn eval_ref(value: &Value, local: &HashMap<String, Value>, document: &HashMap<String, Value>) -> Option<Value> {
    match value {
        Value::String(s) if s.starts_with('$') => {
            let rest = s.trim_start_matches('$');
            if let Some(key) = rest.strip_prefix("local.") {
                local.get(key).cloned()
            } else if let Some(key) = rest.strip_prefix("document.") {
                document.get(key).cloned()
            } else {
                None
            }
        }
        other => Some(other.clone()),
    }
}

fn cmp_f64(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    let a = a.as_f64()?;
    let b = b.as_f64()?;
    a.partial_cmp(&b)
}

fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().unwrap_or(0.0) != 0.0,
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

pub fn interaction_phase_valid(phase: &str) -> bool {
    INTERACTION_PHASES.contains(&phase)
}

pub fn job_state_valid(state: &str) -> bool {
    JOB_STATES.contains(&state)
}

/// Build synthetic document with `count` leaf widgets for limit tests.
pub fn synthetic_node_document(count: u32) -> DeclUiDocument {
    let mut children = Vec::new();
    for i in 0..count.saturating_sub(1) {
        children.push(DeclUiWidget {
            kind: "text".into(),
            text: Some(format!("node-{i}")),
            ..DeclUiWidget::default()
        });
    }
    DeclUiDocument {
        doc_type: "declarative_ui".into(),
        contract: Some(UI_CONTRACT_V2),
        title: "Synthetic".into(),
        title_key: None,
        poll_ms: None,
        labels: None,
        state: None,
        bindings: Vec::new(),
        actions: Vec::new(),
        subscriptions: Vec::new(),
        root: DeclUiWidget {
            kind: "column".into(),
            children: Some(children),
            ..DeclUiWidget::default()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v2_widget_kinds_include_slider_and_image_view() {
        let kinds = widget_kinds_for_contract(UI_CONTRACT_V2);
        assert!(kinds.contains(&"slider"));
        assert!(kinds.contains(&"image_view"));
        assert!(kinds.contains(&"job"));
        assert!(kinds.contains(&"layer_canvas"));
        assert!(kinds.contains(&"layer_list"));
        assert!(kinds.contains(&"undo_redo"));
    }

    #[test]
    fn rejects_contract_above_host_max() {
        let err = validate_ui_contract_supported(HOST_UI_CONTRACT_MAX + 1).unwrap_err();
        assert!(matches!(err, RichDeclUiError::ContractUnsupported(_)));
    }

    #[test]
    fn synthetic_2000_nodes_validates() {
        let doc = synthetic_node_document(MAX_UI_NODES);
        validate_rich_document(&doc, UI_CONTRACT_V2, &[], &[]).expect("2000 nodes ok");
    }

    #[test]
    fn synthetic_2001_nodes_rejected() {
        let doc = synthetic_node_document(MAX_UI_NODES + 1);
        let err = validate_rich_document(&doc, UI_CONTRACT_V2, &[], &[]).unwrap_err();
        assert!(matches!(err, RichDeclUiError::TooManyNodes { .. }));
    }

    #[test]
    fn resolve_local_substitution() {
        let mut local = HashMap::new();
        local.insert("steps".into(), Value::from(5));
        let out = resolve_action_input(&Value::String("$local.steps".into()), &local, &HashMap::new());
        assert_eq!(out, Value::from(5));
    }

    #[test]
    fn predicate_and_eval() {
        let mut local = HashMap::new();
        local.insert("on".into(), Value::Bool(true));
        let pred = serde_json::json!({"and": [{"ref": "$local.on"}, true]});
        validate_predicate(&pred).expect("valid");
        assert!(eval_predicate(&pred, &local, &HashMap::new()));
    }

    #[test]
    fn media_action_requires_cap() {
        let action = RichAction {
            id: "gen".into(),
            tool: None,
            service: Some("media.image.generate".into()),
            input: Some(serde_json::json!({"prompt": "$local.prompt"})),
            refresh_binds: vec![],
            invalidate_on: vec![],
        };
        let err = action.validate(&HashSet::new(), &[]).unwrap_err();
        assert!(matches!(err, RichDeclUiError::MissingCapability(_)));
    }

    #[test]
    fn files_save_as_requires_downloads_read_cap() {
        let action = RichAction {
            id: "save".into(),
            tool: None,
            service: Some("files.save_as".into()),
            input: Some(serde_json::json!({"source_path": "$local.result_path"})),
            refresh_binds: vec![],
            invalidate_on: vec![],
        };
        let caps = vec!["fs.read:/downloads/**".into()];
        action
            .validate(&HashSet::new(), &caps)
            .expect("downloads read cap");
        let err = action.validate(&HashSet::new(), &[]).unwrap_err();
        assert!(matches!(err, RichDeclUiError::MissingCapability(_)));
    }

    #[test]
    fn gallery_demo_ui_document_validates() {
        let raw = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../modules/gallery-demo/ui/index.json"),
        )
        .expect("gallery-demo ui");
        let doc = DeclUiDocument::parse_json_with_contract(raw.as_bytes(), UI_CONTRACT_V2)
            .expect("parse");
        let tools = ["gallery-demo.preview.ensure", "gallery-demo.preview.get"];
        validate_rich_document(&doc, UI_CONTRACT_V2, &tools, &[]).expect("gallery-demo ui valid");
    }

    #[test]
    fn create_ui_document_validates() {
        let raw = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../modules/create/ui/index.json"),
        )
        .expect("create ui");
        let doc = DeclUiDocument::parse_json_with_contract(raw.as_bytes(), UI_CONTRACT_V2)
            .expect("parse");
        let tools = [
            "create.history.list",
            "create.history.get",
            "create.history.record",
            "create.document.load",
            "create.document.save",
            "create.result.get",
        ];
        let caps = vec![
            "media.generate".into(),
            "fs.read:/downloads/**".into(),
            "fs.write:/downloads/**".into(),
            "fs.read:/documents/create/**".into(),
            "fs.write:/documents/create/**".into(),
        ];
        validate_rich_document(&doc, UI_CONTRACT_V2, &tools, &caps).expect("create ui valid");
    }

    #[test]
    fn demo_job_action_allowed_without_media_cap() {
        let action = RichAction {
            id: "run".into(),
            tool: None,
            service: Some("jobs.demo.start".into()),
            input: Some(serde_json::json!({"steps": "$local.steps"})),
            refresh_binds: vec![],
            invalidate_on: vec![],
        };
        action.validate(&HashSet::new(), &[]).expect("demo ok");
    }
}
