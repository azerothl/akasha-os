// SPDX-License-Identifier: Apache-2.0
//! Create module — parameters, generation history, document state (issue #150 lot 2).

use serde::{Deserialize, Serialize};
use serde_json::json;

const HISTORY_PATH: &str = "/documents/create/history.json";
const STATE_PATH: &str = "/documents/create/state.json";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct HistoryStore {
    #[serde(default)]
    items: Vec<HistoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HistoryEntry {
    id: String,
    created_unix: u64,
    path: String,
    prompt: String,
    #[serde(default)]
    model_id: String,
    #[serde(default)]
    engine: String,
    #[serde(default)]
    width: Option<u32>,
    #[serde(default)]
    height: Option<u32>,
    #[serde(default)]
    steps: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct DocumentState {
    #[serde(default)]
    last_result_path: Option<String>,
    #[serde(default)]
    last_prompt: Option<String>,
}

fn handle(tool: &str, args: &serde_json::Value) -> Result<serde_json::Value, String> {
    match tool {
        "create.history.list" => history_list(),
        "create.history.get" => history_get(args),
        "create.history.record" => history_record(args),
        "create.document.load" => document_load(),
        "create.document.save" => document_save(args),
        "create.result.get" => result_get(),
        _ => Err(format!("outil inconnu: {tool}")),
    }
}

fn load_history() -> Result<HistoryStore, String> {
    match aos_module_sdk::fs_read(HISTORY_PATH) {
        Ok(raw) => serde_json::from_str(&raw).map_err(|e| e.to_string()),
        Err(_) => Ok(HistoryStore::default()),
    }
}

fn save_history(store: &HistoryStore) -> Result<(), String> {
    let raw = serde_json::to_string_pretty(store).map_err(|e| e.to_string())?;
    let _ = aos_module_sdk::fs_write(HISTORY_PATH, &raw)?;
    Ok(())
}

fn load_state() -> Result<DocumentState, String> {
    match aos_module_sdk::fs_read(STATE_PATH) {
        Ok(raw) => serde_json::from_str(&raw).map_err(|e| e.to_string()),
        Err(_) => Ok(DocumentState::default()),
    }
}

fn save_state(state: &DocumentState) -> Result<(), String> {
    let raw = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    let _ = aos_module_sdk::fs_write(STATE_PATH, &raw)?;
    Ok(())
}

fn stamp(store: &HistoryStore) -> u64 {
    store
        .items
        .first()
        .map(|e| e.created_unix.saturating_add(1))
        .unwrap_or(1)
}

fn history_list() -> Result<serde_json::Value, String> {
    let store = load_history()?;
    let items: Vec<serde_json::Value> = store
        .items
        .iter()
        .map(|e| {
            json!({
                "id": e.id,
                "created_unix": e.created_unix,
                "path": e.path,
                "prompt": e.prompt,
                "summary": e.prompt.chars().take(48).collect::<String>(),
                "model_id": e.model_id,
            })
        })
        .collect();
    aos_module_sdk::json_ok(&json!({ "items": items }))
}

#[derive(Deserialize)]
struct HistoryGetArgs {
    id: String,
}

fn history_get(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let a: HistoryGetArgs = aos_module_sdk::parse_args(args)?;
    let store = load_history()?;
    let entry = store
        .items
        .iter()
        .find(|e| e.id == a.id)
        .ok_or_else(|| "history entry not found".to_string())?;
    let params = json!({
        "prompt": entry.prompt,
        "model_id": entry.model_id,
        "width": entry.width,
        "height": entry.height,
        "steps": entry.steps,
    });
    aos_module_sdk::json_ok(&json!({
        "entry": entry,
        "params": params,
    }))
}

#[derive(Deserialize)]
struct HistoryRecordArgs {
    path: String,
    prompt: String,
    #[serde(default)]
    model_id: String,
    #[serde(default)]
    engine: String,
    #[serde(default)]
    width: Option<u32>,
    #[serde(default)]
    height: Option<u32>,
    #[serde(default)]
    steps: Option<u32>,
}

fn history_record(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let a: HistoryRecordArgs = aos_module_sdk::parse_args(args)?;
    if a.path.trim().is_empty() || a.prompt.trim().is_empty() {
        return Err("path and prompt required".into());
    }
    let mut store = load_history()?;
    let ts = stamp(&store);
    let id = format!("hist-{ts}");
    let entry = HistoryEntry {
        id,
        created_unix: ts,
        path: a.path,
        prompt: a.prompt,
        model_id: a.model_id,
        engine: a.engine,
        width: a.width,
        height: a.height,
        steps: a.steps,
    };
    store.items.insert(0, entry.clone());
    store.items.truncate(40);
    save_history(&store)?;
    let mut state = load_state()?;
    state.last_result_path = Some(entry.path.clone());
    state.last_prompt = Some(entry.prompt.clone());
    save_state(&state)?;
    aos_module_sdk::json_ok(&json!({ "entry": entry }))
}

fn document_load() -> Result<serde_json::Value, String> {
    let state = load_state()?;
    aos_module_sdk::json_ok(&json!({ "state": state }))
}

fn document_save(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let state: DocumentState = aos_module_sdk::parse_args(args)?;
    save_state(&state)?;
    aos_module_sdk::json_ok(&json!({ "ok": true }))
}

fn result_get() -> Result<serde_json::Value, String> {
    let state = load_state()?;
    let path = state.last_result_path.unwrap_or_default();
    aos_module_sdk::json_ok(&json!({ "path": path }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_entry_serializes() {
        let e = HistoryEntry {
            id: "hist-1".into(),
            created_unix: 1,
            path: "/downloads/image-1.png".into(),
            prompt: "cat".into(),
            model_id: "local:sd".into(),
            engine: "stub".into(),
            width: Some(512),
            height: Some(512),
            steps: Some(12),
        };
        let raw = serde_json::to_string(&e).unwrap();
        assert!(raw.contains("hist-1"));
    }
}

aos_module_sdk::export_module!(handle);
