//! Durable, reviewed proposals from distinct agents on disjoint workspace subtrees.
//! Workers submit text proposals; only a confirmed coordinator applies host edits.
use crate::{storage::StorageFs, workspace::WorkspaceBindManager, workspace_debug};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assignment {
    pub part_id: String,
    pub actor: String,
    pub subtree: String,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Change {
    pub path: String,
    pub base_sha256: String,
    pub content: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub root: String,
    pub plan_id: Option<String>,
    pub part_id: Option<String>,
    pub review_sha256: Option<String>,
    #[serde(default)]
    pub assignments: Vec<Assignment>,
    #[serde(default)]
    pub changes: Vec<Change>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Snapshot {
    path: String,
    before: String,
    after: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Part {
    assignment: Assignment,
    proposal: Option<Vec<Snapshot>>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum State {
    Collecting,
    Ready,
    Applying,
    Applied,
    Undoing,
    Undone,
    RecoveryRequired,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Plan {
    id: String,
    owner: String,
    workspace: String,
    host: PathBuf,
    state: State,
    parts: Vec<Part>,
}

pub fn hash(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}
fn fingerprint(plan: &Plan) -> String {
    format!("{:x}", Sha256::digest(serde_json::to_vec(plan).unwrap()))
}
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temp = path.with_file_name(format!(".aos-refactor-{:032x}.tmp", rand::random::<u128>()));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| e.to_string())?;
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        drop(file);
        std::fs::rename(&temp, path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}
fn forbidden(path: &Path) -> bool {
    path.components().any(|c| {
        let name = c.as_os_str().to_string_lossy().to_ascii_lowercase();
        matches!(
            name.as_str(),
            ".git"
                | "node_modules"
                | "target"
                | "dist"
                | ".env"
                | "credentials.json"
                | "id_rsa"
                | "id_ed25519"
        ) || name.starts_with(".env.")
            || [".pem", ".key", ".p12", ".pfx"]
                .iter()
                .any(|suffix| name.ends_with(suffix))
    })
}
fn checked(root: &Path, raw: &str, directory: bool) -> Result<PathBuf, String> {
    let relative = Path::new(raw);
    if raw.is_empty()
        || raw.contains(['\\', ':', '\0'])
        || raw.chars().any(char::is_control)
        || relative
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
        || forbidden(relative)
    {
        return Err("refactor path is invalid or contains excluded/credential components".into());
    }
    let path = root
        .join(relative)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if !path.starts_with(root)
        || path == root
        || directory != path.is_dir()
        || forbidden(path.strip_prefix(root).unwrap())
    {
        return Err("refactor path escapes its workspace or has the wrong type".into());
    }
    let mut builder = ignore::gitignore::GitignoreBuilder::new(root);
    let mut directories = path
        .parent()
        .unwrap_or(root)
        .ancestors()
        .take_while(|p| p.starts_with(root))
        .collect::<Vec<_>>();
    directories.reverse();
    for directory in directories {
        for name in [".gitignore", ".ignore"] {
            let file = directory.join(name);
            if file.exists() {
                if file
                    .symlink_metadata()
                    .map_err(|e| e.to_string())?
                    .file_type()
                    .is_symlink()
                    || file.metadata().map_err(|e| e.to_string())?.len() > 64 * 1024
                {
                    return Err("ignore rules are symlinked or too large".into());
                }
                if let Some(error) = builder.add(file) {
                    return Err(format!("invalid project ignore rules: {error}"));
                }
            }
        }
    }
    if builder
        .build()
        .map_err(|e| e.to_string())?
        .matched_path_or_any_parents(&path, directory)
        .is_ignore()
    {
        return Err("refactor path is ignored by project rules".into());
    }
    Ok(path)
}
fn read_text(path: &Path) -> Result<String, String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(65537)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 65536 {
        return Err("refactor files are limited to 64 KiB".into());
    }
    String::from_utf8(bytes).map_err(|_| "refactor only accepts UTF-8 text".into())
}
fn manage(id: &str, caps: &[String], write: bool) -> Result<(), String> {
    for kind in if write {
        vec!["refactor.manage", "fs.read", "fs.write"]
    } else {
        vec!["refactor.manage", "fs.read"]
    } {
        let required = format!("{kind}:/host/{id}/**");
        if !caps.contains(&required) {
            return Err(format!("required capability: {required}"));
        }
    }
    Ok(())
}

pub struct RefactorStore {
    path: PathBuf,
    plans: BTreeMap<String, Plan>,
}
impl RefactorStore {
    pub fn open(directory: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
        let path = directory.join("workspace-refactor-plans.json");
        let plans = if path.exists() {
            if path.metadata().map_err(|e| e.to_string())?.len() > 8 * 1024 * 1024 {
                return Err("refactor store exceeds 8 MiB".into());
            }
            serde_json::from_slice(&std::fs::read(&path).map_err(|e| e.to_string())?)
                .map_err(|e| format!("invalid refactor journal: {e}"))?
        } else {
            BTreeMap::new()
        };
        Ok(Self { path, plans })
    }
    fn save(&mut self, plan: Plan) -> Result<(), String> {
        let mut plans = self.plans.clone();
        plans.insert(plan.id.clone(), plan);
        let bytes = serde_json::to_vec(&plans).map_err(|e| e.to_string())?;
        if bytes.len() > 8 * 1024 * 1024 {
            return Err("refactor journal limit reached (8 MiB)".into());
        }
        atomic_write(&self.path, &bytes)?;
        self.plans = plans;
        Ok(())
    }
    fn owned(
        &self,
        mgr: &WorkspaceBindManager,
        req: &Request,
        actor: &str,
    ) -> Result<Plan, String> {
        let (id, root) = workspace_debug::root(mgr, &req.root)?;
        let plan = self
            .plans
            .get(req.plan_id.as_deref().ok_or("plan_id required")?)
            .ok_or("refactor plan unavailable")?;
        if plan.workspace != id || plan.host != root {
            return Err("refactor workspace binding changed".into());
        }
        if plan.owner != actor && actor != "human:ui" {
            return Err("refactor plan belongs to another coordinator".into());
        }
        Ok(plan.clone())
    }
    pub fn create(
        &mut self,
        mgr: &WorkspaceBindManager,
        req: &Request,
        actor: &str,
        caps: &[String],
    ) -> Result<Value, String> {
        let (id, root) = workspace_debug::root(mgr, &req.root)?;
        manage(&id, caps, false)?;
        if self.plans.len() >= 16 {
            return Err(
                "refactor plan limit reached (16); retain journals until reviewed cleanup".into(),
            );
        }
        if !(2..=8).contains(&req.assignments.len()) {
            return Err("assign 2..8 distinct agents to disjoint subtrees".into());
        }
        let mut actors = HashSet::new();
        let mut names = HashSet::new();
        let mut roots = Vec::new();
        for part in &req.assignments {
            if !part.actor.starts_with("agent:")
                || part.actor.len() > 128
                || !actors.insert(part.actor.clone())
                || part.part_id.is_empty()
                || part.part_id.len() > 48
                || !part
                    .part_id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
                || !names.insert(part.part_id.clone())
            {
                return Err("invalid or duplicate refactor assignment".into());
            }
            let path = checked(&root, &part.subtree, true)?;
            if roots
                .iter()
                .any(|existing: &PathBuf| path.starts_with(existing) || existing.starts_with(&path))
            {
                return Err("refactor subtrees overlap".into());
            }
            roots.push(path);
        }
        let plan = Plan {
            id: format!("refactor-{:032x}", rand::random::<u128>()),
            owner: actor.into(),
            workspace: id.clone(),
            host: root,
            state: State::Collecting,
            parts: req
                .assignments
                .iter()
                .cloned()
                .map(|assignment| Part {
                    assignment,
                    proposal: None,
                })
                .collect(),
        };
        self.save(plan.clone())?;
        Ok(
            json!({"ok":true,"plan_id":plan.id,"assignments":plan.parts.iter().map(|part|json!({
            "part_id":part.assignment.part_id,"actor":part.assignment.actor,"subtree":part.assignment.subtree,
            "caps_to_review":[format!("refactor.submit:/host/{id}/{}/**",part.assignment.subtree),
                format!("fs.read:/host/{id}/{}/**",part.assignment.subtree)]})).collect::<Vec<_>>(),
            "execution":"Existing agents submit proposals; no automatic inference or host writes"}),
        )
    }
    pub fn submit(
        &mut self,
        mgr: &WorkspaceBindManager,
        req: &Request,
        actor: &str,
        caps: &[String],
    ) -> Result<Value, String> {
        let (id, root) = workspace_debug::root(mgr, &req.root)?;
        let mut plan = self
            .plans
            .get(req.plan_id.as_deref().ok_or("plan_id required")?)
            .ok_or("refactor plan unavailable")?
            .clone();
        if plan.workspace != id || plan.host != root {
            return Err("refactor workspace binding changed".into());
        }
        if !matches!(plan.state, State::Collecting | State::Ready) {
            return Err("refactor plan no longer accepts proposals".into());
        }
        let part = plan
            .parts
            .iter_mut()
            .find(|p| Some(p.assignment.part_id.as_str()) == req.part_id.as_deref())
            .ok_or("unknown refactor part")?;
        if part.assignment.actor != actor {
            return Err("refactor part belongs to another agent".into());
        }
        let lease = checked(&root, &part.assignment.subtree, true)?;
        if req.changes.len() > 40 {
            return Err("refactor proposal exceeds 40 files".into());
        }
        let mut seen = HashSet::new();
        let mut snapshots = Vec::new();
        for change in &req.changes {
            let path = checked(&root, &change.path, false)?;
            if !path.starts_with(&lease) || !seen.insert(path.clone()) {
                return Err("refactor change escapes assignment or duplicates a target".into());
            }
            let vfs = format!("/host/{id}/{}", change.path);
            StorageFs::check_cap(caps, "fs.read", &vfs).map_err(|e| e.to_string())?;
            StorageFs::check_cap(caps, "refactor.submit", &vfs).map_err(|e| e.to_string())?;
            let before = read_text(&path)?;
            if hash(&before) != change.base_sha256.trim_start_matches("sha256:") {
                return Err("stale proposal: base hash differs".into());
            }
            if change.content.len() > 65536
                || [&before, &change.content]
                    .iter()
                    .any(|text| text.contains("-----BEGIN") && text.contains("PRIVATE KEY-----"))
            {
                return Err("proposal contains an oversized file or private key".into());
            }
            snapshots.push(Snapshot {
                path: change.path.clone(),
                before,
                after: change.content.clone(),
            });
        }
        part.proposal = Some(snapshots);
        if plan
            .parts
            .iter()
            .flat_map(|p| p.proposal.iter().flatten())
            .map(|f| f.before.len() + f.after.len())
            .sum::<usize>()
            > 2 * 1024 * 1024
        {
            return Err("refactor batch exceeds 2 MiB; split the proposal".into());
        }
        plan.state = if plan.parts.iter().all(|p| p.proposal.is_some()) {
            State::Ready
        } else {
            State::Collecting
        };
        self.save(plan.clone())?;
        Ok(json!({"ok":true,"plan_id":plan.id,"state":plan.state,"host_files_written":0}))
    }
    pub fn review(
        &self,
        mgr: &WorkspaceBindManager,
        req: &Request,
        actor: &str,
        caps: &[String],
    ) -> Result<Value, String> {
        let plan = self.owned(mgr, req, actor)?;
        manage(&plan.workspace, caps, false)?;
        let mut diff = String::new();
        for part in &plan.parts {
            for file in part.proposal.iter().flatten() {
                diff.push_str(&format!(
                    "--- a/{}\n+++ b/{}\n@@ -1,{} +1,{} @@\n",
                    file.path,
                    file.path,
                    file.before.lines().count(),
                    file.after.lines().count()
                ));
                for line in file.before.lines() {
                    diff.push('-');
                    diff.push_str(line);
                    diff.push('\n');
                }
                for line in file.after.lines() {
                    diff.push('+');
                    diff.push_str(line);
                    diff.push('\n');
                }
            }
        }
        Ok(
            json!({"ok":true,"plan_id":plan.id,"state":plan.state,"review_sha256":fingerprint(&plan),"diff":diff,
            "can_apply":plan.state==State::Ready,"can_undo":matches!(plan.state,State::Applying|State::Applied|State::Undoing|State::RecoveryRequired),
            "parts":plan.parts}),
        )
    }
    pub fn apply(
        &mut self,
        mgr: &WorkspaceBindManager,
        req: &Request,
        actor: &str,
        caps: &[String],
        undo: bool,
    ) -> Result<Value, String> {
        let mut plan = self.owned(mgr, req, actor)?;
        manage(&plan.workspace, caps, true)?;
        if req.review_sha256.as_deref() != Some(fingerprint(&plan).as_str()) {
            return Err("review changed; inspect the plan again".into());
        }
        if (!undo && plan.state != State::Ready)
            || (undo
                && !matches!(
                    plan.state,
                    State::Applying | State::Applied | State::Undoing | State::RecoveryRequired
                ))
        {
            return Err("refactor plan is not in an applicable/recoverable state".into());
        }
        let files = plan
            .parts
            .iter()
            .flat_map(|p| p.proposal.clone().unwrap_or_default())
            .collect::<Vec<_>>();
        if files
            .iter()
            .map(|f| f.before.len() + f.after.len())
            .sum::<usize>()
            > 2 * 1024 * 1024
        {
            return Err("refactor batch exceeds 2 MiB".into());
        }
        let mut targets = Vec::new();
        for file in &files {
            let path = checked(&plan.host, &file.path, false)?;
            let current = read_text(&path)?;
            if current != file.before && (!undo || current != file.after) {
                return Err(format!("external edit in {}; no files changed", file.path));
            }
            targets.push(path);
        }
        plan.state = if undo {
            State::Undoing
        } else {
            State::Applying
        };
        self.save(plan.clone())?; // Durable before-images before the first host write.
        let mut changed: Vec<usize> = Vec::new();
        for (index, file) in files.iter().enumerate() {
            let expected = if undo { &file.after } else { &file.before };
            let desired = if undo { &file.before } else { &file.after };
            let result = read_text(&targets[index]).and_then(|current| {
                if undo && current == file.before {
                    return Ok(());
                }
                if current != *expected {
                    return Err("file changed during application".into());
                }
                atomic_write(&targets[index], desired.as_bytes())
            });
            if let Err(error) = result {
                let mut unresolved = Vec::new();
                if !undo {
                    for previous in changed.iter().rev().copied() {
                        let snapshot: &Snapshot = &files[previous];
                        if read_text(&targets[previous]).is_ok_and(|s| s == snapshot.after) {
                            if atomic_write(&targets[previous], snapshot.before.as_bytes()).is_err()
                            {
                                unresolved.push(snapshot.path.clone());
                            }
                        } else {
                            unresolved.push(snapshot.path.clone());
                        }
                    }
                }
                plan.state = if !undo && unresolved.is_empty() {
                    State::Ready
                } else {
                    State::RecoveryRequired
                };
                let _ = self.save(plan.clone());
                return Err(format!("refactor write failed: {error}; rollback_unresolved={unresolved:?}; review plan {}",plan.id));
            }
            changed.push(index);
        }
        plan.state = if undo { State::Undone } else { State::Applied };
        self.save(plan.clone()).map_err(|e| {
            format!(
                "files written; journal finalization failed: {e}; review recovery before retrying"
            )
        })?;
        Ok(
            json!({"ok":true,"plan_id":plan.id,"state":plan.state,"files":files.iter().map(|f|&f.path).collect::<Vec<_>>(),
            "review_sha256":fingerprint(&plan)}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture {
        home: PathBuf,
        mgr: WorkspaceBindManager,
        store: RefactorStore,
        caps: Vec<String>,
        req: Request,
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.home);
        }
    }
    impl Fixture {
        fn new() -> Self {
            let home = std::env::temp_dir()
                .join(format!("aos-refactor-test-{:032x}", rand::random::<u128>()));
            for dir in ["project/front", "project/back", "project/front/nested"] {
                std::fs::create_dir_all(home.join(dir)).unwrap();
            }
            for file in ["front/a.rs", "back/b.rs"] {
                std::fs::write(home.join("project").join(file), "before\n").unwrap();
            }
            let mut mgr = WorkspaceBindManager::open(home.join("sessions")).unwrap();
            mgr.bind(
                &home.join("project").to_string_lossy(),
                Some("demo"),
                "test",
            )
            .unwrap();
            let store = RefactorStore::open(&home.join("sessions")).unwrap();
            let caps = ["refactor.manage", "fs.read", "fs.write"]
                .iter()
                .map(|k| format!("{k}:/host/demo/**"))
                .collect();
            let req = serde_json::from_value(json!({"root":"demo", "assignments":[
                {"part_id":"front","actor":"agent:front","subtree":"front"},
                {"part_id":"back","actor":"agent:back","subtree":"back"}]}))
            .unwrap();
            Self {
                home,
                mgr,
                store,
                caps,
                req,
            }
        }
        fn create(&mut self) {
            let result = self
                .store
                .create(&self.mgr, &self.req, "agent:lead", &self.caps)
                .unwrap();
            self.req.plan_id = Some(result["plan_id"].as_str().unwrap().into());
        }
        fn submit(&mut self, part: &str) {
            let file = if part == "front" {
                "front/a.rs"
            } else {
                "back/b.rs"
            };
            self.req.part_id = Some(part.into());
            self.req.changes = vec![Change {
                path: file.into(),
                base_sha256: hash("before\n"),
                content: format!("after {part}\n"),
            }];
            let caps = ["refactor.submit", "fs.read"]
                .iter()
                .map(|k| format!("{k}:/host/demo/{part}/**"))
                .collect::<Vec<_>>();
            self.store
                .submit(&self.mgr, &self.req, &format!("agent:{part}"), &caps)
                .unwrap();
        }
        fn review(&mut self) -> Value {
            let result = self
                .store
                .review(&self.mgr, &self.req, "human:ui", &self.caps)
                .unwrap();
            self.req.review_sha256 = Some(result["review_sha256"].as_str().unwrap().into());
            result
        }
    }
    #[test]
    fn two_agents_propose_without_writes_then_review_apply_restart_and_undo() {
        let mut f = Fixture::new();
        f.create();
        f.submit("front");
        assert_eq!(f.review()["state"], "collecting");
        assert!(f
            .store
            .apply(&f.mgr, &f.req, "human:ui", &f.caps, false)
            .is_err());
        f.submit("back");
        assert_eq!(f.review()["state"], "ready");
        assert_eq!(
            std::fs::read_to_string(f.home.join("project/front/a.rs")).unwrap(),
            "before\n"
        );
        f.store
            .apply(&f.mgr, &f.req, "human:ui", &f.caps, false)
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(f.home.join("project/front/a.rs")).unwrap(),
            "after front\n"
        );
        f.store = RefactorStore::open(&f.home.join("sessions")).unwrap();
        assert_eq!(f.review()["state"], "applied");
        f.store
            .apply(&f.mgr, &f.req, "human:ui", &f.caps, true)
            .unwrap();
        for file in ["front/a.rs", "back/b.rs"] {
            assert_eq!(
                std::fs::read_to_string(f.home.join("project").join(file)).unwrap(),
                "before\n"
            );
        }
        assert_eq!(f.review()["state"], "undone");
    }
    #[test]
    fn identities_scoped_grants_and_overlapping_assignments_fail_closed() {
        let mut f = Fixture::new();
        f.req.assignments[1].subtree = "front/nested".into();
        assert!(f
            .store
            .create(&f.mgr, &f.req, "agent:lead", &f.caps)
            .unwrap_err()
            .contains("overlap"));
        f.req.assignments[1].subtree = "back".into();
        f.create();
        f.submit("front");
        assert!(f
            .store
            .submit(&f.mgr, &f.req, "agent:back", &f.caps)
            .unwrap_err()
            .contains("another agent"));
        assert!(f.store.submit(&f.mgr, &f.req, "agent:front", &[]).is_err());
        assert!(f
            .store
            .review(&f.mgr, &f.req, "agent:other", &f.caps)
            .is_err());
        f.req.changes[0].path = "back/b.rs".into();
        assert!(f
            .store
            .submit(
                &f.mgr,
                &f.req,
                "agent:front",
                &[
                    "fs.read:/host/demo/**".into(),
                    "refactor.submit:/host/demo/**".into()
                ]
            )
            .unwrap_err()
            .contains("escapes assignment"));
    }
    #[test]
    fn ignored_credentials_traversal_and_nested_ignore_rules_are_excluded() {
        let f = Fixture::new();
        let root = f.home.join("project").canonicalize().unwrap();
        std::fs::write(root.join(".gitignore"), "back/\n").unwrap();
        std::fs::write(root.join("front/.ignore"), "*.secret\n").unwrap();
        for file in ["front/token.secret", "front/.env", "front/key.pem"] {
            std::fs::write(root.join(file), "private").unwrap();
        }
        for file in [
            "back/b.rs",
            "front/token.secret",
            "front/.env",
            "front/key.pem",
            "../outside",
            "front/../back/b.rs",
        ] {
            assert!(checked(&root, file, false).is_err(), "{file}");
        }
        assert!(checked(&root, "front/a.rs", false).is_ok());
    }
    #[test]
    fn stale_review_and_external_edits_never_overwrite_files() {
        let mut f = Fixture::new();
        f.create();
        f.submit("front");
        f.submit("back");
        f.review();
        let stale = f.req.review_sha256.clone();
        f.req.changes[0].content = "changed proposal\n".into();
        f.store
            .submit(
                &f.mgr,
                &f.req,
                "agent:back",
                &[
                    "fs.read:/host/demo/back/**".into(),
                    "refactor.submit:/host/demo/back/**".into(),
                ],
            )
            .unwrap();
        assert_eq!(stale, f.req.review_sha256);
        assert!(f
            .store
            .apply(&f.mgr, &f.req, "human:ui", &f.caps, false)
            .unwrap_err()
            .contains("review changed"));
        f.review();
        std::fs::write(f.home.join("project/back/b.rs"), "external\n").unwrap();
        assert!(f
            .store
            .apply(&f.mgr, &f.req, "human:ui", &f.caps, false)
            .unwrap_err()
            .contains("external edit"));
        assert_eq!(
            std::fs::read_to_string(f.home.join("project/front/a.rs")).unwrap(),
            "before\n"
        );
    }
    #[test]
    fn interrupted_application_can_be_reviewed_and_undone_but_preserves_external_edits() {
        let mut f = Fixture::new();
        f.create();
        f.submit("front");
        f.submit("back");
        let mut plan = f.store.plans[f.req.plan_id.as_ref().unwrap()].clone();
        plan.state = State::Applying;
        f.store.save(plan).unwrap();
        std::fs::write(f.home.join("project/front/a.rs"), "after front\n").unwrap();
        f.store = RefactorStore::open(&f.home.join("sessions")).unwrap();
        assert_eq!(f.review()["can_undo"], true);
        std::fs::write(f.home.join("project/back/b.rs"), "external\n").unwrap();
        assert!(f
            .store
            .apply(&f.mgr, &f.req, "human:ui", &f.caps, true)
            .is_err());
        assert_eq!(
            std::fs::read_to_string(f.home.join("project/front/a.rs")).unwrap(),
            "after front\n"
        );
        std::fs::write(f.home.join("project/back/b.rs"), "before\n").unwrap();
        f.store
            .apply(&f.mgr, &f.req, "human:ui", &f.caps, true)
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(f.home.join("project/front/a.rs")).unwrap(),
            "before\n"
        );
    }
    #[test]
    fn stale_base_private_key_and_changed_workspace_are_rejected() {
        let mut f = Fixture::new();
        f.create();
        f.submit("front");
        let caps = [
            "fs.read:/host/demo/front/**".into(),
            "refactor.submit:/host/demo/front/**".into(),
        ];
        f.req.changes[0].base_sha256 = hash("stale");
        assert!(f
            .store
            .submit(&f.mgr, &f.req, "agent:front", &caps)
            .unwrap_err()
            .contains("stale"));
        f.req.changes[0].base_sha256 = hash("before\n");
        f.req.changes[0].content = "-----BEGIN RSA PRIVATE KEY-----".into();
        assert!(f
            .store
            .submit(&f.mgr, &f.req, "agent:front", &caps)
            .unwrap_err()
            .contains("private key"));
        f.req.root = "another".into();
        assert!(f
            .store
            .review(&f.mgr, &f.req, "agent:lead", &f.caps)
            .is_err());
    }
}
