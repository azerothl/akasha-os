//! Durable source-file map and safe explicit context reads for bound projects.
use crate::{
    storage::StorageFs,
    workspace_policy::{self, ScanLimits, WorkspacePolicy},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{io::Write, path::Path};
const POLICY_VERSION: u32 = 2;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub root: String,
    pub path: Option<String>,
    pub max_files: Option<usize>,
    pub max_bytes: Option<u64>,
    pub resume: Option<bool>,
    pub page_entries: Option<usize>,
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn stored(cache: &Path, root: &Path) -> Result<Value, String> {
    if cache
        .metadata()
        .map_err(|_| "no inventory; run workspace.inspect first")?
        .len()
        > 8 * 1024 * 1024
    {
        return Err("inventory cache exceeds 8 MiB".into());
    }
    let value: Value = serde_json::from_slice(&std::fs::read(cache).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if value["binding"] != json!(root) || value["policy_version"] != POLICY_VERSION {
        return Err("inventory binding/policy changed; inspect again".into());
    }
    if value["files"]
        .as_array()
        .is_none_or(|files| files.len() > workspace_policy::MAX_SCAN_FILES)
    {
        return Err("invalid inventory cache".into());
    }
    Ok(value)
}
fn save(cache: &Path, value: &Value) -> Result<(), String> {
    let data = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    if data.len() > 8 * 1024 * 1024 {
        return Err("inventory exceeds 8 MiB".into());
    }
    std::fs::create_dir_all(cache.parent().ok_or("cache parent missing")?)
        .map_err(|e| e.to_string())?;
    let temporary = cache.with_file_name(format!(
        ".aos-inventory-{:032x}.tmp",
        rand::random::<u128>()
    ));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        file.write_all(&data)
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        drop(file);
        std::fs::rename(&temporary, cache).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Continuation {
    pending: Vec<String>,
    files: usize,
    bytes: u64,
    page_entries: usize,
    started_ms: u64,
    scan_id: String,
    pages: usize,
    coverage_gap: bool,
}
fn now_ms() -> Result<u64, String> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis() as u64)
}
fn cache_lock(path: &Path) -> Result<std::sync::Arc<std::sync::Mutex<()>>, String> {
    use std::sync::{Arc, Mutex, OnceLock, Weak};
    type Locks = std::collections::HashMap<std::path::PathBuf, Weak<Mutex<()>>>;
    static LOCKS: OnceLock<Mutex<Locks>> = OnceLock::new();
    let mut locks = LOCKS
        .get_or_init(|| Mutex::new(Locks::new()))
        .lock()
        .map_err(|_| "inspection locks unavailable")?;
    locks.retain(|_, lock| lock.strong_count() != 0);
    if let Some(lock) = locks.get(path).and_then(Weak::upgrade) {
        return Ok(lock);
    }
    if locks.len() >= 128 {
        return Err("too many concurrent workspace inspections".into());
    }
    let lock = Arc::new(Mutex::new(()));
    locks.insert(path.to_path_buf(), Arc::downgrade(&lock));
    Ok(lock)
}
fn public(mut value: Value, cached: bool) -> Value {
    let obj = value.as_object_mut().unwrap();
    obj.remove("binding");
    obj.remove("continuation_state");
    let interrupted = obj
        .remove("page_in_progress")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if interrupted {
        value["continuation"]["available"] = json!(false);
        value["continuation"]["reason"] = json!("interrupted_page; start a new inspection");
    }

    value["cached"] = json!(cached);
    value["freshness"] = json!(if cached {
        "not_rechecked; use workspace.inspect to refresh or continue"
    } else {
        "observed_across_bounded_pages; earlier pages not rechecked; not an atomic filesystem snapshot"
    });
    value
}
fn merge_scan(
    before: Option<&Value>,
    next: &workspace_policy::ScanReport,
    done: bool,
    gap: bool,
) -> Value {
    let mut merged = serde_json::to_value(next).expect("scan report serializes");
    if let Some(old) = before {
        for key in [
            "files_read",
            "eligible_files",
            "bytes_read",
            "visited_entries",
            "finding_files",
        ] {
            merged[key] = json!(old[key].as_u64().unwrap_or(0) + merged[key].as_u64().unwrap_or(0));
        }
        let mut skipped = old["skipped"].as_object().cloned().unwrap_or_default();
        for (key, count) in merged["skipped"].as_object().unwrap() {
            let previous = skipped.get(key).and_then(Value::as_u64).unwrap_or(0);
            skipped.insert(key.clone(), json!(previous + count.as_u64().unwrap_or(0)));
        }
        merged["skipped"] = Value::Object(skipped);
        let mut findings = old["findings"].as_array().cloned().unwrap_or_default();
        findings.extend(merged["findings"].as_array().unwrap().iter().cloned());
        findings.truncate(256);
        merged["findings"] = json!(findings);
    }
    merged["findings_truncated"] = json!(merged["finding_files"].as_u64().unwrap_or(0) > 256);
    merged["complete"] = json!(done && !gap);
    merged
}
/// The service resolves actor/root capabilities before calling this implementation.
pub fn execute(
    root: &Path,
    id: &str,
    cache_dir: &Path,
    intent: &str,
    request: &Request,
    caps: &[String],
) -> Result<Value, String> {
    let mut policy = WorkspacePolicy::new(root)?;
    let root = policy.root().to_path_buf();
    if intent == "workspace.read" {
        let relative = request
            .path
            .as_deref()
            .ok_or("relative project path required")?;
        if !workspace_policy::secret_findings(relative).is_empty() {
            return Err("source path withheld by credential policy".into());
        }
        StorageFs::check_cap(caps, "fs.read", &format!("/host/{id}/{relative}"))
            .map_err(|e| e.to_string())?;
        let content = policy.read_text(relative)?;
        if content.len() > 65536 {
            return Err("safe source reads are limited to 64 KiB".into());
        }
        return Ok(
            json!({"ok":true,"path":format!("/host/{id}/{relative}"),"sha256":hash(content.as_bytes()),"content":content,"policy_version":POLICY_VERSION}),
        );
    }
    let required = format!("fs.read:/host/{id}/**");
    if !caps.contains(&required) {
        return Err(format!("required capability: {required}"));
    }
    let cache = cache_dir.join(format!("workspace-inventory-{id}.json"));
    let lock = cache_lock(&cache)?;
    let _guard = lock
        .lock()
        .map_err(|_| "inspection cache lock unavailable")?;
    if intent == "workspace.inventory" {
        return Ok(public(stored(&cache, &root)?, true));
    }
    if intent != "workspace.inspect" {
        return Err("unknown workspace inspection operation".into());
    }
    let now = now_ms()?;
    let resumed = request.resume.unwrap_or(false);
    let previous = if resumed {
        Some(stored(&cache, &root)?)
    } else {
        stored(&cache, &root).ok()
    };
    let mut state = if resumed {
        let saved = previous.as_ref().unwrap();
        if saved["page_in_progress"] == true {
            return Err("inspection page interrupted; start a new inspection".into());
        }

        let state: Continuation = serde_json::from_value(saved["continuation_state"].clone())
            .map_err(|_| "no resumable inspection; start a new inspection")?;
        for (key, ceiling) in [
            ("files_read", 10_000),
            ("eligible_files", 10_000),
            ("bytes_read", workspace_policy::MAX_SCAN_BYTES + 1),
            ("visited_entries", 50_000),
            ("finding_files", 10_000),
        ] {
            if saved["scan"][key].as_u64().is_none_or(|n| n > ceiling) {
                return Err("invalid inspection continuation counters".into());
            }
        }
        let eligible = saved["scan"]["eligible_files"].as_u64().unwrap();
        let findings = saved["scan"]["finding_files"].as_u64().unwrap();
        let reads = saved["scan"]["files_read"].as_u64().unwrap();
        if saved["files"].as_array().unwrap().len() as u64 != eligible
            || eligible + findings > reads
            || saved["scan"]["skipped"].as_object().is_none_or(|map| {
                map.len() > 32
                    || map
                        .values()
                        .any(|n| n.as_u64().is_none_or(|n| n > 1_000_000_000))
            })
        {
            return Err("invalid inspection continuation metadata".into());
        }
        if now < state.started_ms
            || now - state.started_ms > 3_600_000
            || state.pending.is_empty()
            || !(1..=workspace_policy::MAX_SCAN_FILES).contains(&state.files)
            || !(1..=workspace_policy::MAX_SCAN_BYTES).contains(&state.bytes)
            || !(1..=50_000).contains(&state.page_entries)
            || state.pages >= 50_000
        {
            return Err("inspection continuation expired or invalid; inspect again".into());
        }
        if request
            .max_files
            .is_some_and(|v| v.clamp(1, workspace_policy::MAX_SCAN_FILES) != state.files)
            || request
                .max_bytes
                .is_some_and(|v| v.clamp(1, workspace_policy::MAX_SCAN_BYTES) != state.bytes)
            || request
                .page_entries
                .is_some_and(|v| v.clamp(1, 5000) != state.page_entries)
        {
            return Err("continuation limits cannot change; start a new inspection".into());
        }
        state
    } else {
        Continuation {
            pending: vec![],
            files: request
                .max_files
                .unwrap_or(workspace_policy::MAX_SCAN_FILES)
                .clamp(1, workspace_policy::MAX_SCAN_FILES),
            bytes: request
                .max_bytes
                .unwrap_or(workspace_policy::MAX_SCAN_BYTES)
                .clamp(1, workspace_policy::MAX_SCAN_BYTES),
            page_entries: request
                .page_entries
                .map(|n| n.clamp(1, 5000))
                .unwrap_or(50_000),
            started_ms: now,
            scan_id: format!("scan-{:032x}", rand::random::<u128>()),
            pages: 0,
            coverage_gap: false,
        }
    };
    let old_scan = if resumed {
        previous.as_ref().map(|v| &v["scan"])
    } else {
        None
    };
    let consumed = |key: &str| -> u64 { old_scan.and_then(|v| v[key].as_u64()).unwrap_or(0) };
    let seen_files = consumed("files_read");
    let seen_bytes = consumed("bytes_read");
    let seen_entries = consumed("visited_entries");
    if seen_files >= state.files as u64 || seen_bytes >= state.bytes || seen_entries >= 50_000 {
        return Err("inspection total budget exhausted; start a new inspection".into());
    }
    let mut files = if resumed {
        previous.as_ref().unwrap()["files"]
            .as_array()
            .unwrap()
            .clone()
    } else {
        vec![]
    };
    // Persist a claim before reading the page. If a process dies or saving the
    // result fails, the claim refuses replay of unaccounted work on this job.
    // Restarting an interrupted job is explicit; completed pages remain resumable.
    let mut lease_state = serde_json::to_value(&state).map_err(|e| e.to_string())?;
    if !resumed {
        lease_state["pending"] = json!([""]);
    }
    let mut lease_scan = old_scan
        .cloned()
        .unwrap_or_else(|| serde_json::to_value(workspace_policy::ScanReport::default()).unwrap());
    lease_scan["complete"] = json!(false);
    let lease = json!({"ok":true,"workspace_id":id,"namespace":format!("workspace:{id}:source-map:v1"),"binding":root,
        "policy_version":POLICY_VERSION,"scanned_at_ms":previous.as_ref().and_then(|v|v["scanned_at_ms"].as_u64()).unwrap_or(now),
        "content_version":hash(&serde_json::to_vec(&files).map_err(|e|e.to_string())?),"files":files,"scan":lease_scan,
        "page_in_progress":true,"attempt_started_at_ms":now,"continuation_state":lease_state,
        "continuation":{"available":false,"reason":"incomplete_page","scan_id":state.scan_id,"pages":state.pages,
            "started_at_ms":state.started_ms,"expires_at_ms":state.started_ms+3_600_000},
        "delta":{"available":false,"reason":"incomplete page"}});
    save(&cache, &lease)?;
    let page = workspace_policy::scan_page(
        &mut policy,
        &root,
        ScanLimits {
            files: state.files - seen_files as usize,
            bytes: state.bytes - seen_bytes,
        },
        if resumed {
            Some(std::mem::take(&mut state.pending))
        } else {
            None
        },
        workspace_policy::PageBudget {
            duration: std::time::Duration::from_secs(10),
            entries: state.page_entries.min(50_000 - seen_entries as usize),
        },
        |path, text| {
            files.push(json!({"path":path,"bytes":text.len(),"lines":text.lines().count(),"language":Path::new(path).extension().and_then(|s|s.to_str()).unwrap_or("text"),"sha256":hash(text.as_bytes())}));
            true
        },
    )?;
    state.pending = page.pending;
    state.pages += 1;
    state.coverage_gap |= page.coverage_gap;
    let total_files = seen_files + page.report.files_read as u64;
    let total_bytes = seen_bytes + page.report.bytes_read;
    let total_entries = seen_entries + page.report.visited_entries as u64;
    let budget_exhausted = !state.pending.is_empty()
        && (total_files >= state.files as u64
            || total_bytes >= state.bytes
            || total_entries >= 50_000);
    if budget_exhausted {
        state.pending.clear();
        state.coverage_gap = true;
    }
    let done = state.pending.is_empty();
    let mut report = merge_scan(old_scan, &page.report, done, state.coverage_gap);
    if budget_exhausted {
        report["skipped"]["total_budget"] =
            json!(report["skipped"]["total_budget"].as_u64().unwrap_or(0) + 1);
    }
    let content_version = hash(&serde_json::to_vec(&files).map_err(|e| e.to_string())?);
    let delta = if !resumed
        && report["complete"] == true
        && previous
            .as_ref()
            .is_some_and(|v| v["scan"]["complete"] == true)
    {
        let before = previous.as_ref().unwrap()["files"].as_array().unwrap();
        let old = before
            .iter()
            .map(|f| (f["path"].as_str().unwrap_or(""), &f["sha256"]))
            .collect::<std::collections::BTreeMap<_, _>>();
        let new = files
            .iter()
            .map(|f| (f["path"].as_str().unwrap_or(""), &f["sha256"]))
            .collect::<std::collections::BTreeMap<_, _>>();
        json!({"available":true,"added":new.keys().filter(|p|!old.contains_key(**p)).collect::<Vec<_>>(),
            "changed":new.iter().filter(|(p,h)|old.get(**p).is_some_and(|old|*old!=**h)).map(|(p,_)|*p).collect::<Vec<_>>(),
            "no_longer_eligible":old.keys().filter(|p|!new.contains_key(**p)).collect::<Vec<_>>()})
    } else {
        json!({"available":false,"reason":"first, partial or resumed scan, or changed binding/policy"})
    };
    let mut value = json!({"ok":true,"workspace_id":id,"namespace":format!("workspace:{id}:source-map:v1"),"binding":root,
        "policy_version":POLICY_VERSION,"scanned_at_ms":now_ms()?,"content_version":content_version,"files":files,"scan":report,
        "continuation":{"available":!done,"reason":if !done {"page_boundary"} else if state.coverage_gap {"coverage_limited"} else {"walk_complete"},"scan_id":state.scan_id,"pages":state.pages,"started_at_ms":state.started_ms,"expires_at_ms":state.started_ms+3_600_000},
        "delta":delta});
    if !done {
        value["continuation_state"] = serde_json::to_value(&state).map_err(|e| e.to_string())?;
    }
    save(&cache, &value)?;
    let value = public(value, false);
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    struct Fixture(PathBuf);
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn req() -> Request {
        serde_json::from_value(json!({"root":"demo"})).unwrap()
    }
    #[test]
    fn inventory_survives_reload_tracks_changes_and_never_persists_source_or_secret_values() {
        let home = Fixture(
            std::env::temp_dir().join(format!("aos-inventory-{:032x}", rand::random::<u128>())),
        );
        let root = home.0.join("project");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("safe.rs"), "fn safe() {}\n").unwrap();
        let token = format!("ghp_{}", "T".repeat(36));
        std::fs::write(
            root.join("blocked.rs"),
            format!("const token = \"{token}\";\n"),
        )
        .unwrap();
        let cache = home.0.join("sessions");
        let caps = vec!["fs.read:/host/demo/**".into()];
        let first = execute(&root, "demo", &cache, "workspace.inspect", &req(), &caps).unwrap();
        assert_eq!(first["scan"]["finding_files"], 1);
        assert_eq!(first["files"].as_array().unwrap().len(), 1);
        let disk = std::fs::read_to_string(cache.join("workspace-inventory-demo.json")).unwrap();
        assert!(!disk.contains(&token));
        assert!(!disk.contains("fn safe()"));
        let loaded = execute(&root, "demo", &cache, "workspace.inventory", &req(), &caps).unwrap();
        assert_eq!(loaded["content_version"], first["content_version"]);
        assert_eq!(loaded["cached"], true);
        assert!(loaded.get("binding").is_none());
        std::fs::write(root.join("safe.rs"), "fn changed() {}\n").unwrap();
        std::fs::write(root.join("new.py"), "answer=42\n").unwrap();
        let next = execute(&root, "demo", &cache, "workspace.inspect", &req(), &caps).unwrap();
        assert_eq!(next["delta"]["changed"], json!(["safe.rs"]));
        assert_eq!(next["delta"]["added"], json!(["new.py"]));
        let partial = Request {
            max_files: Some(1),
            ..req()
        };
        let next = execute(&root, "demo", &cache, "workspace.inspect", &partial, &caps).unwrap();
        assert_eq!(next["scan"]["complete"], false);
        assert_eq!(next["delta"]["available"], false);
        let other = home.0.join("rebound");
        std::fs::create_dir(&other).unwrap();
        assert!(
            execute(&other, "demo", &cache, "workspace.inventory", &req(), &caps)
                .unwrap_err()
                .contains("binding/policy changed")
        );
    }
    #[test]
    fn safe_read_enforces_subtree_cap_and_blocks_ignored_credentials_and_traversal() {
        let home = Fixture(
            std::env::temp_dir().join(format!("aos-safe-read-{:032x}", rand::random::<u128>())),
        );
        let root = home.0.join("project");
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(root.join(".ignore"), "ignored.rs\n").unwrap();
        std::fs::write(root.join("ignored.rs"), "hidden\n").unwrap();
        let cache = home.0.join("sessions");
        let request = Request {
            path: Some("src/main.rs".into()),
            ..req()
        };
        assert!(execute(&root, "demo", &cache, "workspace.read", &request, &[]).is_err());
        let caps = vec!["fs.read:/host/demo/src/**".into()];
        let result = execute(&root, "demo", &cache, "workspace.read", &request, &caps).unwrap();
        assert_eq!(result["content"], "fn main() {}\n");
        for path in ["../outside", "ignored.rs", ".env"] {
            let request = Request {
                path: Some(path.into()),
                ..req()
            };
            assert!(execute(
                &root,
                "demo",
                &cache,
                "workspace.read",
                &request,
                &["fs.read:/host/demo/**".into()]
            )
            .is_err());
        }
        assert!(execute(&root, "demo", &cache, "workspace.inspect", &req(), &caps).is_err());
    }
    fn project(count: usize) -> Fixture {
        let home = Fixture(
            std::env::temp_dir().join(format!("aos-resume-{:032x}", rand::random::<u128>())),
        );
        std::fs::create_dir_all(home.0.join("project")).unwrap();
        for i in 0..count {
            std::fs::write(
                home.0.join(format!("project/file-{i:03}.rs")),
                format!("fn source_{i}() {{}}\n"),
            )
            .unwrap();
        }
        home
    }
    fn paged(entries: usize) -> Request {
        serde_json::from_value(json!({"root":"demo","page_entries":entries})).unwrap()
    }
    fn resume() -> Request {
        serde_json::from_value(json!({"root":"demo","resume":true})).unwrap()
    }
    #[test]
    fn resumed_inventory_reloads_private_cursor_and_matches_one_pass_without_secret_values() {
        let home = project(8);
        let root = home.0.join("project");
        let cache = home.0.join("sessions");
        let caps = vec!["fs.read:/host/demo/**".into()];
        let token = format!("ghp_{}", "X".repeat(36));
        std::fs::write(root.join(format!("{token}.rs")), "hidden-name").unwrap();
        std::fs::write(root.join("secret.rs"), &token).unwrap();
        let full = execute(
            &root,
            "demo",
            &cache,
            "workspace.inspect",
            &paged(5000),
            &caps,
        )
        .unwrap();
        let mut result =
            execute(&root, "demo", &cache, "workspace.inspect", &paged(2), &caps).unwrap();
        let scan_id = result["continuation"]["scan_id"].clone();
        assert_eq!(result["continuation"]["available"], true);
        assert!(execute(&root, "demo", &cache, "workspace.inspect", &resume(), &[]).is_err());
        while result["continuation"]["available"] == true {
            let cached =
                execute(&root, "demo", &cache, "workspace.inventory", &req(), &caps).unwrap();
            assert!(cached.get("continuation_state").is_none());
            assert!(!cached.to_string().contains(&token));
            assert!(
                !std::fs::read_to_string(cache.join("workspace-inventory-demo.json"))
                    .unwrap()
                    .contains(&token)
            );
            result = execute(&root, "demo", &cache, "workspace.inspect", &resume(), &caps).unwrap();
            assert_eq!(result["continuation"]["scan_id"], scan_id);
            assert!(result["continuation"]["pages"].as_u64().unwrap() < 20);
        }
        assert_eq!(result["scan"]["complete"], true);
        assert_eq!(result["files"], full["files"]);
        assert_eq!(result["content_version"], full["content_version"]);
        assert_eq!(result["scan"]["files_read"], full["scan"]["files_read"]);
        assert_eq!(result["scan"]["finding_files"], 1);
        assert_eq!(result["delta"]["available"], false);
        assert!(execute(&root, "demo", &cache, "workspace.inspect", &resume(), &caps).is_err());
    }
    #[test]
    fn resumed_inventory_cannot_raise_or_reset_total_budgets() {
        let home = project(8);
        let root = home.0.join("project");
        let cache = home.0.join("sessions");
        let caps = vec!["fs.read:/host/demo/**".into()];
        let mut request = paged(2);
        request.max_files = Some(3);
        let mut result =
            execute(&root, "demo", &cache, "workspace.inspect", &request, &caps).unwrap();
        let mut changed = resume();
        changed.max_files = Some(100);
        assert!(
            execute(&root, "demo", &cache, "workspace.inspect", &changed, &caps)
                .unwrap_err()
                .contains("limits cannot change")
        );
        while result["continuation"]["available"] == true {
            result = execute(&root, "demo", &cache, "workspace.inspect", &resume(), &caps).unwrap();
        }
        assert_eq!(result["scan"]["files_read"], 3);
        assert_eq!(result["scan"]["complete"], false);
        assert_eq!(result["continuation"]["available"], false);
        assert!(execute(&root, "demo", &cache, "workspace.inspect", &resume(), &caps).is_err());
        let mut request = paged(1);
        request.max_bytes = Some(34);
        let mut result =
            execute(&root, "demo", &cache, "workspace.inspect", &request, &caps).unwrap();
        while result["continuation"]["available"] == true {
            result = execute(&root, "demo", &cache, "workspace.inspect", &resume(), &caps).unwrap();
        }
        assert!(result["scan"]["bytes_read"].as_u64().unwrap() <= 34);
        assert_eq!(result["scan"]["files_read"], 2);
        assert_eq!(result["scan"]["complete"], false);
    }
    #[test]
    fn expired_rebound_and_corrupt_continuations_are_refused() {
        let home = project(3);
        let root = home.0.join("project");
        let cache = home.0.join("sessions");
        let caps = vec!["fs.read:/host/demo/**".into()];
        let filename = cache.join("workspace-inventory-demo.json");
        execute(&root, "demo", &cache, "workspace.inspect", &paged(1), &caps).unwrap();
        let mut saved: Value = serde_json::from_slice(&std::fs::read(&filename).unwrap()).unwrap();
        saved["continuation_state"]["started_ms"] = json!(0);
        std::fs::write(&filename, saved.to_string()).unwrap();
        assert!(
            execute(&root, "demo", &cache, "workspace.inspect", &resume(), &caps)
                .unwrap_err()
                .contains("expired")
        );
        execute(&root, "demo", &cache, "workspace.inspect", &paged(1), &caps).unwrap();
        let mut saved: Value = serde_json::from_slice(&std::fs::read(&filename).unwrap()).unwrap();
        saved["continuation_state"]["pending"] = json!(["../outside"]);
        std::fs::write(&filename, saved.to_string()).unwrap();
        assert!(
            execute(&root, "demo", &cache, "workspace.inspect", &resume(), &caps)
                .unwrap_err()
                .contains("continuation path")
        );
        let other = home.0.join("other");
        std::fs::create_dir(&other).unwrap();
        assert!(execute(
            &other,
            "demo",
            &cache,
            "workspace.inspect",
            &resume(),
            &caps
        )
        .unwrap_err()
        .contains("binding/policy changed"));
    }
    #[test]
    fn simultaneous_continuations_advance_once_per_page() {
        let home = project(60);
        let root = home.0.join("project");
        let cache = home.0.join("sessions");
        let caps = vec!["fs.read:/host/demo/**".into()];
        let full = execute(
            &root,
            "demo",
            &cache,
            "workspace.inspect",
            &paged(5000),
            &caps,
        )
        .unwrap();
        execute(&root, "demo", &cache, "workspace.inspect", &paged(1), &caps).unwrap();
        std::thread::scope(|scope| {
            for _ in 0..8 {
                let (root, cache, caps) = (&root, &cache, &caps);
                scope.spawn(move || {
                    for _ in 0..5 {
                        execute(root, "demo", cache, "workspace.inspect", &resume(), caps).unwrap();
                    }
                });
            }
        });
        let mut result =
            execute(&root, "demo", &cache, "workspace.inventory", &req(), &caps).unwrap();
        assert_eq!(result["continuation"]["pages"], 41);
        assert_eq!(result["scan"]["files_read"], 40);
        while result["continuation"]["available"] == true {
            result = execute(&root, "demo", &cache, "workspace.inspect", &resume(), &caps).unwrap();
        }
        assert_eq!(result["files"], full["files"]);
        assert_eq!(result["scan"]["files_read"], 60);
        assert_eq!(result["scan"]["complete"], true);
    }
    #[test]
    fn resumed_inventory_enforces_global_entry_cap_and_refuses_missing_counters() {
        let home = project(3);
        let root = home.0.join("project");
        let cache = home.0.join("sessions");
        let caps = vec!["fs.read:/host/demo/**".into()];
        let filename = cache.join("workspace-inventory-demo.json");
        execute(&root, "demo", &cache, "workspace.inspect", &paged(1), &caps).unwrap();
        let saved: Value = serde_json::from_slice(&std::fs::read(&filename).unwrap()).unwrap();
        let mut corrupt = saved.clone();
        corrupt["scan"]
            .as_object_mut()
            .unwrap()
            .remove("files_read");
        std::fs::write(&filename, corrupt.to_string()).unwrap();
        assert!(
            execute(&root, "demo", &cache, "workspace.inspect", &resume(), &caps)
                .unwrap_err()
                .contains("counters")
        );
        let mut near_limit = saved;
        near_limit["scan"]["visited_entries"] = json!(49_999);
        std::fs::write(&filename, near_limit.to_string()).unwrap();
        let result = execute(&root, "demo", &cache, "workspace.inspect", &resume(), &caps).unwrap();
        assert_eq!(result["scan"]["visited_entries"], 50_000);
        assert_eq!(result["scan"]["complete"], false);
        assert_eq!(result["scan"]["skipped"]["total_budget"], 1);
        assert_eq!(result["continuation"]["available"], false);
    }
    #[test]
    fn interrupted_page_claim_cannot_be_replayed_and_requires_explicit_restart() {
        let home = project(3);
        let root = home.0.join("project");
        let cache = home.0.join("sessions");
        let caps = vec!["fs.read:/host/demo/**".into()];
        let filename = cache.join("workspace-inventory-demo.json");
        execute(&root, "demo", &cache, "workspace.inspect", &paged(1), &caps).unwrap();
        let mut saved: Value = serde_json::from_slice(&std::fs::read(&filename).unwrap()).unwrap();
        saved["page_in_progress"] = json!(true);
        std::fs::write(&filename, saved.to_string()).unwrap();
        assert!(
            execute(&root, "demo", &cache, "workspace.inspect", &resume(), &caps)
                .unwrap_err()
                .contains("interrupted")
        );
        let cached = execute(&root, "demo", &cache, "workspace.inventory", &req(), &caps).unwrap();
        assert_eq!(cached["continuation"]["available"], false);
        assert!(cached.get("page_in_progress").is_none());
        let fresh = execute(
            &root,
            "demo",
            &cache,
            "workspace.inspect",
            &paged(5000),
            &caps,
        )
        .unwrap();
        assert_eq!(fresh["scan"]["complete"], true);
        assert_eq!(fresh["scan"]["files_read"], 3);
    }
    #[test]
    fn failed_page_marks_the_job_unreplayable_before_any_retry() {
        let home = project(3);
        let root = home.0.join("project");
        let cache = home.0.join("sessions");
        let caps = vec!["fs.read:/host/demo/**".into()];
        execute(&root, "demo", &cache, "workspace.inspect", &paged(1), &caps).unwrap();
        std::fs::write(root.join(".ignore"), "x".repeat(65_537)).unwrap();
        assert!(execute(&root, "demo", &cache, "workspace.inspect", &resume(), &caps).is_err());
        std::fs::remove_file(root.join(".ignore")).unwrap();
        assert!(
            execute(&root, "demo", &cache, "workspace.inspect", &resume(), &caps)
                .unwrap_err()
                .contains("interrupted")
        );
        let cached = execute(&root, "demo", &cache, "workspace.inventory", &req(), &caps).unwrap();
        assert_eq!(cached["scan"]["complete"], false);
        assert_eq!(cached["continuation"]["available"], false);
        let fresh = execute(
            &root,
            "demo",
            &cache,
            "workspace.inspect",
            &paged(5000),
            &caps,
        )
        .unwrap();
        assert_eq!(fresh["scan"]["complete"], true);
    }
}
