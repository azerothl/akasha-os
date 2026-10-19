//! Durable source-file map and safe explicit context reads for bound projects.
use crate::{
    storage::StorageFs,
    workspace_policy::{self, ScanLimits, WorkspacePolicy},
};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{io::Write, path::Path};
const POLICY_VERSION: u32 = 1;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub root: String,
    pub path: Option<String>,
    pub max_files: Option<usize>,
    pub max_bytes: Option<u64>,
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
    if intent == "workspace.inventory" {
        let mut result = stored(&cache, &root)?;
        result.as_object_mut().unwrap().remove("binding");
        result["cached"] = json!(true);
        result["freshness"] = json!("not_rechecked; use workspace.inspect to refresh");
        return Ok(result);
    }
    if intent != "workspace.inspect" {
        return Err("unknown workspace inspection operation".into());
    }
    let previous = stored(&cache, &root).ok();
    let mut files = Vec::new();
    let report = workspace_policy::scan(
        &mut policy,
        &root,
        ScanLimits {
            files: request
                .max_files
                .unwrap_or(workspace_policy::MAX_SCAN_FILES),
            bytes: request
                .max_bytes
                .unwrap_or(workspace_policy::MAX_SCAN_BYTES),
        },
        |path, text| {
            files.push(json!({"path":path,"bytes":text.len(),"lines":text.lines().count(),"language":Path::new(path).extension().and_then(|s|s.to_str()).unwrap_or("text"),"sha256":hash(text.as_bytes())}));
            true
        },
    )?;
    let content_version = hash(&serde_json::to_vec(&files).map_err(|e| e.to_string())?);
    let delta = if report.complete
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
        json!({"available":false,"reason":"first scan, changed binding/policy, or partial scan"})
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis();
    let mut value = json!({"ok":true,"workspace_id":id,"namespace":format!("workspace:{id}:source-map:v1"),"binding":root,
        "policy_version":POLICY_VERSION,"scanned_at_ms":now,"content_version":content_version,"files":files,"scan":report});
    save(&cache, &value)?;
    value.as_object_mut().unwrap().remove("binding");
    value["delta"] = delta;
    value["cached"] = json!(false);
    value["freshness"] = json!("observed_during_bounded_scan; not an atomic filesystem snapshot");
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
}
