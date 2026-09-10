// SPDX-License-Identifier: Apache-2.0
//! Create module — parameters, generation history, document state (issue #150 lot 2).

use serde::{Deserialize, Serialize};
use serde_json::json;

const HISTORY_PATH: &str = "/documents/create/history.json";
const STATE_PATH: &str = "/documents/create/state.json";

#[derive(Debug, Clone, Copy)]
struct CatalogPack {
    id: &'static str,
    label: &'static str,
    modality: &'static str,
}

/// Media packs mirrored from `share/models/catalog-offerings.json` (image + video).
const MEDIA_PACKS: &[CatalogPack] = &[
    CatalogPack {
        id: "local:sd-v1-5",
        label: "Stable Diffusion 1.5",
        modality: "image",
    },
    CatalogPack {
        id: "local:flux2",
        label: "Flux 2-class",
        modality: "image",
    },
    CatalogPack {
        id: "local:ideogram4",
        label: "Ideogram 4",
        modality: "image",
    },
    CatalogPack {
        id: "local:sdxl-base",
        label: "Stable Diffusion XL 1.0",
        modality: "image",
    },
    CatalogPack {
        id: "local:z-image-turbo",
        label: "Z-Image Turbo",
        modality: "image",
    },
    CatalogPack {
        id: "local:qwen-image-2512",
        label: "Qwen Image 2512",
        modality: "image",
    },
    CatalogPack {
        id: "local:krea2-raw",
        label: "Krea 2 Base",
        modality: "image",
    },
    CatalogPack {
        id: "local:wan2.2-t2i",
        label: "Wan 2.2 T2V (single frame)",
        modality: "image",
    },
    CatalogPack {
        id: "local:ltx2.3-dev",
        label: "LTX 2.3 Dev",
        modality: "video",
    },
    CatalogPack {
        id: "local:minimax-h3",
        label: "MiniMax H3",
        modality: "video",
    },
];

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct HistoryStore {
    #[serde(default)]
    items: Vec<HistoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HistoryEntry {
    id: String,
    created_unix: u64,
    /// Human-facing timestamp for table chrome (never raw unix in UI).
    when: String,
    path: String,
    prompt: String,
    #[serde(default)]
    model_id: String,
    #[serde(default)]
    engine: String,
    #[serde(default)]
    media_mode: String,
    #[serde(default)]
    width: Option<u32>,
    #[serde(default)]
    height: Option<u32>,
    #[serde(default)]
    steps: Option<u32>,
    /// Full Create state snapshot so restoring history brings back advanced
    /// controls (references, adapters, sampling and composition), not just
    /// the three legacy dimensions.
    #[serde(default)]
    params: serde_json::Value,
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
        "create.models.list" => models_list(),
        _ => Err(format!("outil inconnu: {tool}")),
    }
}

fn models_list() -> Result<serde_json::Value, String> {
    let image: Vec<serde_json::Value> = MEDIA_PACKS
        .iter()
        .filter(|p| p.modality == "image")
        .map(|p| json!({ "id": p.id, "label": p.label }))
        .collect();
    let video: Vec<serde_json::Value> = MEDIA_PACKS
        .iter()
        .filter(|p| p.modality == "video")
        .map(|p| json!({ "id": p.id, "label": p.label }))
        .collect();
    aos_module_sdk::json_ok(&json!({
        "image": image,
        "video": video,
        "default_image": "local:sd-v1-5",
        "default_video": "local:ltx2.3-dev",
    }))
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

fn utc_date_time_label(secs: u64) -> String {
    if secs == 0 {
        return "—".into();
    }
    let z = secs / 86_400;
    let time = secs % 86_400;
    let h = time / 3600;
    let m = (time % 3600) / 60;
    let (y, mo, d) = days_to_ymd(z);
    format!("{:04}-{:02}-{:02} {:02}:{:02}", y, mo, d, h, m)
}

fn days_to_ymd(z: u64) -> (u64, u64, u64) {
    let z = z + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mp < 10 { y } else { y + 1 };
    (y, m, d)
}

fn history_list() -> Result<serde_json::Value, String> {
    let store = load_history()?;
    let items: Vec<serde_json::Value> = store
        .items
        .iter()
        .map(|e| {
            json!({
                "id": e.id,
                "path": e.path,
                "prompt": e.prompt,
                "summary": e.prompt.chars().take(48).collect::<String>(),
                "when": e.when,
                "model_id": e.model_id,
                "media_mode": e.media_mode,
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
    let mut params = json!({
        "prompt": entry.prompt,
        "model_id": entry.model_id,
        "media_mode": entry.media_mode,
        "width": entry.width,
        "height": entry.height,
        "steps": entry.steps,
        "result_path": entry.path,
    });
    if let (Some(base), Some(extra)) = (params.as_object_mut(), entry.params.as_object()) {
        for (key, value) in extra {
            base.insert(key.clone(), value.clone());
        }
    }
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
    media_mode: String,
    #[serde(default)]
    width: Option<u32>,
    #[serde(default)]
    height: Option<u32>,
    #[serde(default)]
    steps: Option<u32>,
    #[serde(default)]
    params: serde_json::Value,
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
        when: utc_date_time_label(ts),
        path: a.path,
        prompt: a.prompt,
        model_id: a.model_id,
        engine: a.engine,
        media_mode: a.media_mode,
        width: a.width,
        height: a.height,
        steps: a.steps,
        params: a.params,
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
            when: "1970-01-01 00:00".into(),
            path: "/downloads/image-1.png".into(),
            prompt: "cat".into(),
            model_id: "local:sd".into(),
            engine: "sdcpp".into(),
            media_mode: "image".into(),
            width: Some(512),
            height: Some(512),
            steps: Some(12),
            params: serde_json::Value::Null,
        };
        let raw = serde_json::to_string(&e).unwrap();
        assert!(raw.contains("hist-1"));
    }

    #[test]
    fn models_list_includes_image_and_video_modes() {
        let out = models_list().expect("models");
        let image = out.get("image").and_then(|v| v.as_array()).expect("image");
        let video = out.get("video").and_then(|v| v.as_array()).expect("video");
        assert!(!image.is_empty());
        assert!(!video.is_empty());
        assert!(
            image.iter().any(|row| row.get("id").and_then(|v| v.as_str()) == Some("local:sd-v1-5"))
        );
        assert!(
            video
                .iter()
                .any(|row| row.get("id").and_then(|v| v.as_str()) == Some("local:ltx2.3-dev"))
        );
    }
}

aos_module_sdk::export_module!(handle);
