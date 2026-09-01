// SPDX-License-Identifier: Apache-2.0
//! Module « tasks » — second dual-surface artefact (Preview 0.3 / E3).
//!
//! Tools: `tasks.create` / `tasks.list` / `tasks.update` / `tasks.complete`
//! Storage: JSON list under `/documents/tasks/tasks.json`.

use serde::{Deserialize, Serialize};
use serde_json::json;

const TASKS_PATH: &str = "/documents/tasks/tasks.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Task {
    id: String,
    title: String,
    #[serde(default)]
    notes: String,
    #[serde(default)]
    done: bool,
    created_ms: u64,
    #[serde(default)]
    updated_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct TaskStore {
    #[serde(default)]
    tasks: Vec<Task>,
}

fn handle(tool: &str, args: &serde_json::Value) -> Result<serde_json::Value, String> {
    match tool {
        "tasks.create" => create(args),
        "tasks.list" => list(),
        "tasks.update" => update(args),
        "tasks.complete" => complete(args),
        _ => Err(format!("outil inconnu: {tool}")),
    }
}

fn stamp(store: &TaskStore) -> u64 {
    let n = store.tasks.len() as u64;
    store
        .tasks
        .last()
        .map(|t| t.created_ms.saturating_add(1).max(n + 1))
        .unwrap_or(n + 1)
}

fn load() -> Result<TaskStore, String> {
    match aos_module_sdk::fs_read(TASKS_PATH) {
        Ok(raw) => serde_json::from_str(&raw).map_err(|e| e.to_string()),
        Err(_) => Ok(TaskStore::default()),
    }
}

fn save(store: &TaskStore) -> Result<(), String> {
    let raw = serde_json::to_string_pretty(store).map_err(|e| e.to_string())?;
    let _ = aos_module_sdk::fs_write(TASKS_PATH, &raw)?;
    Ok(())
}

#[derive(Deserialize)]
struct CreateArgs {
    title: String,
    #[serde(default)]
    notes: String,
}

fn create(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let a: CreateArgs = aos_module_sdk::parse_args(args)?;
    if a.title.trim().is_empty() {
        return Err("title vide".into());
    }
    let mut store = load()?;
    let ts = stamp(&store);
    let task = Task {
        id: format!("task-{ts}"),
        title: a.title.trim().to_string(),
        notes: a.notes,
        done: false,
        created_ms: ts,
        updated_ms: ts,
    };
    store.tasks.push(task.clone());
    save(&store)?;
    aos_module_sdk::json_ok(&json!({ "task": task }))
}

fn list() -> Result<serde_json::Value, String> {
    let store = load()?;
    aos_module_sdk::json_ok(&json!({ "tasks": store.tasks }))
}

#[derive(Deserialize)]
struct UpdateArgs {
    id: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default)]
    done: Option<bool>,
}

fn update(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let a: UpdateArgs = aos_module_sdk::parse_args(args)?;
    let mut store = load()?;
    let next = stamp(&store);
    let task = store
        .tasks
        .iter_mut()
        .find(|t| t.id == a.id)
        .ok_or_else(|| format!("task introuvable: {}", a.id))?;
    if let Some(title) = a.title.filter(|t| !t.trim().is_empty()) {
        task.title = title;
    }
    if let Some(notes) = a.notes {
        task.notes = notes;
    }
    if let Some(done) = a.done {
        task.done = done;
    }
    task.updated_ms = task.updated_ms.saturating_add(1).max(next);
    let out = task.clone();
    save(&store)?;
    aos_module_sdk::json_ok(&json!({ "task": out }))
}

#[derive(Deserialize)]
struct CompleteArgs {
    id: String,
    #[serde(default = "default_true")]
    done: bool,
}

fn default_true() -> bool {
    true
}

fn complete(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let a: CompleteArgs = aos_module_sdk::parse_args(args)?;
    update(&json!({
        "id": a.id,
        "done": a.done,
    }))
}

aos_module_sdk::export_module!(handle);
