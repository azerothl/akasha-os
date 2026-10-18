//! Explicit host execution for bound projects (#247). No shell or client argv.
//! The administrator allowlists fixed commands per workspace. Every execution
//! additionally requires scoped capabilities and a live host confirmation.
use crate::workspace::WorkspaceBindManager;
use aos_proto::workspace::{
    GitCommitRequest, WorkspaceProblem, WorkspaceTaskRequest, WorkspaceTaskResponse,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt};

const OUTPUT_LIMIT: usize = 256 * 1024;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandPolicy {
    pub workspaces: HashMap<String, HashMap<String, AllowedCommand>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AllowedCommand {
    /// Absolute executable; scripts must name their interpreter explicitly.
    pub program: PathBuf,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default = "default_timeout")]
    pub timeout_sec: u64,
}
fn default_timeout() -> u64 {
    120
}

#[derive(Debug, Clone)]
pub struct PreparedCommand {
    pub workspace_id: String,
    pub cwd: PathBuf,
    pub program: PathBuf,
    pub args: Vec<String>,
    pub timeout_sec: u64,
    pub verify_git_root: bool,
}

fn bound_root(
    mgr: &WorkspaceBindManager,
    root: &str,
    caps: &[String],
    kind: &str,
) -> Result<(String, PathBuf), String> {
    let id = if root.starts_with("/host/") {
        let id = aos_proto::workspace::workspace_id_from_vfs_path(root)
            .ok_or("invalid workspace root")?;
        if root.trim_end_matches('/') != format!("/host/{id}") {
            return Err("execution requires the bound workspace root".into());
        }
        id
    } else {
        root.to_string()
    };
    let host = mgr.resolve_host_path(&id).ok_or("workspace is not bound")?;
    for cap in [
        format!("{kind}:/host/{id}/**"),
        format!("fs.read:/host/{id}/**"),
        format!("fs.write:/host/{id}/**"),
    ] {
        if !caps.contains(&cap) {
            return Err(format!("required capability: {cap}"));
        }
    }
    let cwd = PathBuf::from(host)
        .canonicalize()
        .map_err(|e| format!("workspace unavailable: {e}"))?;
    if !cwd.is_dir() {
        return Err("workspace root is not a directory".into());
    }
    Ok((id, cwd))
}

pub fn prepare_task(
    mgr: &WorkspaceBindManager,
    req: &WorkspaceTaskRequest,
    policy_path: &Path,
) -> Result<PreparedCommand, String> {
    let (id, cwd) = bound_root(mgr, &req.root, &req.caps, "process.run")?;
    let data = std::fs::read(policy_path)
        .map_err(|_| "process.run is disabled: no workspace-process-policy.json".to_string())?;
    let policy: CommandPolicy =
        serde_json::from_slice(&data).map_err(|e| format!("invalid process policy: {e}"))?;
    let command = policy
        .workspaces
        .get(&id)
        .and_then(|tasks| tasks.get(&req.task))
        .ok_or("task is not allowlisted for this workspace")?;
    if !command.program.is_absolute() || !command.program.is_file() {
        return Err("allowlisted program must be an existing absolute executable".into());
    }
    if command.args.len() > 128
        || command
            .args
            .iter()
            .any(|arg| arg.len() > 32_768 || arg.contains('\0'))
    {
        return Err("invalid allowlisted arguments".into());
    }
    Ok(PreparedCommand {
        workspace_id: id,
        cwd,
        program: command.program.canonicalize().map_err(|e| e.to_string())?,
        args: command.args.clone(),
        timeout_sec: command.timeout_sec.clamp(1, 300),
        verify_git_root: false,
    })
}

pub fn prepare_commit(
    mgr: &WorkspaceBindManager,
    req: &GitCommitRequest,
) -> Result<PreparedCommand, String> {
    let (id, cwd) = bound_root(mgr, &req.root, &req.caps, "git.commit")?;
    if req.message.trim().is_empty() || req.message.len() > 4096 || req.message.contains('\0') {
        return Err("commit message must contain 1..4096 bytes".into());
    }
    if !cwd.join(".git").exists() {
        return Err("bind the Git repository root before committing".into());
    }
    let filename = if cfg!(windows) { "git.exe" } else { "git" };
    let program = std::env::var_os("PATH")
        .and_then(|path| {
            std::env::split_paths(&path)
                .filter(|part| part.is_absolute())
                .map(|part| part.join(filename))
                .find(|candidate| candidate.is_file())
        })
        .ok_or("git executable not found on PATH")?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    // Stage nothing automatically, and disable host hooks, signing subprocesses,
    // fsmonitor and external diff helpers for this narrowly scoped commit path.
    let args = [
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "commit.gpgsign=false",
        "-c",
        "core.fsmonitor=false",
        "commit",
        "--no-verify",
        "--no-gpg-sign",
        "-m",
        &req.message,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    Ok(PreparedCommand {
        workspace_id: id,
        cwd,
        program,
        args,
        timeout_sec: 60,
        verify_git_root: true,
    })
}

async fn drain(mut pipe: impl AsyncRead + Unpin) -> (String, bool) {
    let mut kept = Vec::new();
    let mut truncated = false;
    let mut block = [0u8; 8192];
    loop {
        match pipe.read(&mut block).await {
            Ok(0) | Err(_) => break,
            Ok(size) => {
                let remaining = OUTPUT_LIMIT.saturating_sub(kept.len());
                kept.extend_from_slice(&block[..size.min(remaining)]);
                truncated |= size > remaining;
            }
        }
    }
    (String::from_utf8_lossy(&kept).into_owned(), truncated)
}

pub async fn execute(command: PreparedCommand) -> Result<WorkspaceTaskResponse, String> {
    if command.verify_git_root {
        let mut probe = command.clone();
        probe.verify_git_root = false;
        probe.timeout_sec = 5;
        probe.args = ["-c", "core.fsmonitor=false", "rev-parse", "--show-toplevel"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let result = execute_fixed(probe).await?;
        let actual = PathBuf::from(result.stdout.trim())
            .canonicalize()
            .map_err(|_| "cannot verify Git worktree root".to_string())?;
        if !result.ok || actual != command.cwd {
            return Err("Git worktree root is outside the bound workspace".into());
        }
    }
    execute_fixed(command).await
}

async fn execute_fixed(command: PreparedCommand) -> Result<WorkspaceTaskResponse, String> {
    let mut child_cmd = tokio::process::Command::new(&command.program);
    child_cmd
        .args(&command.args)
        .current_dir(&command.cwd)
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    for key in [
        "PATH",
        "SystemRoot",
        "WINDIR",
        "TEMP",
        "TMP",
        "HOME",
        "USERPROFILE",
        "LOCALAPPDATA",
        "APPDATA",
        "CARGO_HOME",
        "RUSTUP_HOME",
    ] {
        if let Some(value) = std::env::var_os(key) {
            child_cmd.env(key, value);
        }
    }
    child_cmd
        .env("CARGO_NET_OFFLINE", "true")
        .env("GIT_TERMINAL_PROMPT", "0");
    #[cfg(windows)]
    child_cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    #[cfg(unix)]
    child_cmd.process_group(0);
    let mut child = child_cmd
        .spawn()
        .map_err(|e| format!("command launch failed: {e}"))?;
    let group = ProcessGroup::new(&child)?;
    let stdout_task = tokio::spawn(drain(child.stdout.take().ok_or("stdout unavailable")?));
    let stderr_task = tokio::spawn(drain(child.stderr.take().ok_or("stderr unavailable")?));
    let (status, timed_out) =
        match tokio::time::timeout(Duration::from_secs(command.timeout_sec), child.wait()).await {
            Ok(result) => (Some(result.map_err(|e| e.to_string())?), false),
            Err(_) => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                (None, true)
            }
        };
    // Closing the owned job/group also stops descendants that kept pipes open.
    drop(group);
    let (stdout, a) = finish_drain(stdout_task).await?;
    let (stderr, b) = finish_drain(stderr_task).await?;
    let problems = parse_problems(
        &format!("{stdout}\n{stderr}"),
        &command.workspace_id,
        &command.cwd,
    );
    let exit_code = status.and_then(|s| s.code());
    Ok(WorkspaceTaskResponse {
        ok: !timed_out && exit_code == Some(0),
        exit_code,
        stdout,
        stderr,
        timed_out,
        truncated: a || b,
        problems,
    })
}

async fn finish_drain(
    mut task: tokio::task::JoinHandle<(String, bool)>,
) -> Result<(String, bool), String> {
    match tokio::time::timeout(Duration::from_secs(1), &mut task).await {
        Ok(result) => result.map_err(|e| e.to_string()),
        Err(_) => {
            // A host program can deliberately escape a process group. Do not
            // hang forever if it leaves a descendant holding an inherited pipe.
            task.abort();
            Ok(("[output capture timed out after process exit]".into(), true))
        }
    }
}

#[cfg(windows)]
pub(crate) struct ProcessGroup(isize);
#[cfg(windows)]
impl ProcessGroup {
    pub(crate) fn new(child: &tokio::process::Child) -> Result<Self, String> {
        use windows::Win32::{
            Foundation::{CloseHandle, HANDLE},
            System::JobObjects::*,
        };
        unsafe {
            let job = CreateJobObjectW(None, None).map_err(|e| e.to_string())?;
            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if let Err(error) = SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const std::ffi::c_void,
                std::mem::size_of_val(&limits) as u32,
            ) {
                let _ = CloseHandle(job);
                return Err(error.to_string());
            }
            let Some(raw) = child.raw_handle() else {
                let _ = CloseHandle(job);
                return Err("child handle unavailable".into());
            };
            if let Err(error) = AssignProcessToJobObject(job, HANDLE(raw)) {
                let _ = CloseHandle(job);
                return Err(format!("cannot supervise process tree: {error}"));
            }
            Ok(Self(job.0 as isize))
        }
    }
}
#[cfg(windows)]
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(windows::Win32::Foundation::HANDLE(
                self.0 as *mut std::ffi::c_void,
            ));
        }
    }
}

#[cfg(unix)]
pub(crate) struct ProcessGroup(i32);
#[cfg(unix)]
impl ProcessGroup {
    pub(crate) fn new(child: &tokio::process::Child) -> Result<Self, String> {
        child
            .id()
            .map(|id| Self(id as i32))
            .ok_or("child id unavailable".into())
    }
}
#[cfg(unix)]
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        unsafe {
            libc::kill(-self.0, libc::SIGKILL);
        }
    }
}

/// Cargo/rustc JSON diagnostics only; unrecognized tool output stays in stdout.
pub fn parse_problems(output: &str, id: &str, root: &Path) -> Vec<WorkspaceProblem> {
    let mut problems = Vec::new();
    for line in output.lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let message = if value["reason"] == "compiler-message" {
            &value["message"]
        } else {
            &value
        };
        let Some(level) = message["level"]
            .as_str()
            .filter(|s| ["error", "warning", "note"].contains(s))
        else {
            continue;
        };
        let Some(span) = message["spans"]
            .as_array()
            .and_then(|spans| spans.iter().find(|s| s["is_primary"] == true))
        else {
            continue;
        };
        let Some(filename) = span["file_name"].as_str() else {
            continue;
        };
        let path = Path::new(filename);
        let rel = if path.is_absolute() {
            match path.strip_prefix(root) {
                Ok(p) => p,
                Err(_) => continue,
            }
        } else {
            path
        };
        if rel.components().any(|c| !matches!(c, Component::Normal(_))) {
            continue;
        }
        problems.push(WorkspaceProblem {
            path: format!("/host/{id}/{}", rel.to_string_lossy().replace('\\', "/")),
            line: span["line_start"].as_u64().unwrap_or(1),
            column: span["column_start"].as_u64().unwrap_or(1),
            severity: level.into(),
            message: message["message"]
                .as_str()
                .unwrap_or("")
                .chars()
                .take(2000)
                .collect(),
        });
        if problems.len() == 500 {
            break;
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "aos-workspace-exec-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            assert!(self.0.starts_with(std::env::temp_dir()));
            assert!(self
                .0
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("aos-workspace-exec-"));
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn caps(kind: &str) -> Vec<String> {
        vec![
            format!("{kind}:/host/project/**"),
            "fs.read:/host/project/**".into(),
            "fs.write:/host/project/**".into(),
        ]
    }
    fn binding(temp: &Temp) -> WorkspaceBindManager {
        let mut mgr = WorkspaceBindManager::open(temp.0.join("sessions")).unwrap();
        mgr.bind(temp.0.to_str().unwrap(), Some("project"), "agent:test")
            .unwrap();
        mgr
    }
    fn child_command(temp: &Temp, fixture: &str, timeout: u64) -> PreparedCommand {
        PreparedCommand {
            workspace_id: "project".into(),
            cwd: temp.0.clone(),
            program: std::env::current_exe().unwrap(),
            args: vec![
                "--exact".into(),
                format!("workspace_process::tests::{fixture}"),
                "--nocapture".into(),
            ],
            timeout_sec: timeout,
            verify_git_root: false,
        }
    }
    fn is_child(name: &str) -> bool {
        std::env::args().any(|arg| arg == format!("workspace_process::tests::{name}"))
    }

    #[test]
    fn output_child_fixture() {
        if is_child("output_child_fixture") {
            print!("{}", "x".repeat(OUTPUT_LIMIT * 3));
        }
    }
    #[test]
    fn slow_child_fixture() {
        if is_child("slow_child_fixture") {
            std::thread::sleep(Duration::from_secs(10));
        }
    }
    #[test]
    #[allow(clippy::zombie_processes)] // The enclosing job/group must reap this deliberately inherited child.
    fn descendant_child_fixture() {
        if is_child("descendant_child_fixture") {
            let _child = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "workspace_process::tests::slow_child_fixture",
                    "--nocapture",
                ])
                .spawn()
                .unwrap();
        }
    }

    #[tokio::test]
    async fn real_process_output_is_drained_with_bounded_memory() {
        let temp = Temp::new();
        let result = execute(child_command(&temp, "output_child_fixture", 5))
            .await
            .unwrap();
        assert!(result.ok);
        assert!(result.truncated);
        assert!(result.stdout.len() <= OUTPUT_LIMIT + 3);
    }
    #[tokio::test]
    async fn deadline_and_descendant_cleanup_are_bounded() {
        let temp = Temp::new();
        let started = std::time::Instant::now();
        let result = execute(child_command(&temp, "slow_child_fixture", 1))
            .await
            .unwrap();
        assert!(result.timed_out);
        assert!(!result.ok);
        assert!(started.elapsed() < Duration::from_secs(4));
        let started = std::time::Instant::now();
        let result = execute(child_command(&temp, "descendant_child_fixture", 3))
            .await
            .unwrap();
        assert!(result.ok);
        assert!(started.elapsed() < Duration::from_secs(4));
    }
    #[test]
    fn policy_and_scoped_caps_are_required_and_client_argv_is_refused() {
        let temp = Temp::new();
        let mgr = binding(&temp);
        let policy = temp.0.join("policy.json");
        let mut req = WorkspaceTaskRequest {
            root: "project".into(),
            task: "check".into(),
            actor: "agent:test".into(),
            caps: vec![],
            trace_id: "test".into(),
        };
        assert!(prepare_task(&mgr, &req, &policy)
            .unwrap_err()
            .contains("capability"));
        req.caps = caps("process.run");
        assert!(prepare_task(&mgr, &req, &policy)
            .unwrap_err()
            .contains("disabled"));
        std::fs::write(
            &policy,
            serde_json::to_vec(&serde_json::json!({"workspaces":{"project":{"check":{
            "program":std::env::current_exe().unwrap(),"args":["--help"],"timeout_sec":10}}}}))
            .unwrap(),
        )
        .unwrap();
        let prepared = prepare_task(&mgr, &req, &policy).unwrap();
        assert_eq!(prepared.args, ["--help"]);
        req.root = "/host/project/../outside".into();
        assert!(prepare_task(&mgr, &req, &policy).is_err());
        assert!(serde_json::from_value::<WorkspaceTaskRequest>(
            serde_json::json!({"root":"project","task":"check","program":"evil","args":["--shell"]})
        )
        .is_err());
    }
    #[tokio::test]
    async fn commit_keeps_unstaged_work_and_disables_hooks() {
        let temp = Temp::new();
        let mgr = binding(&temp);
        let git = |args: &[&str]| {
            Command::new("git")
                .args(args)
                .current_dir(&temp.0)
                .output()
                .unwrap()
        };
        assert!(git(&["init"]).status.success());
        git(&["config", "user.name", "Akasha test"]);
        git(&["config", "user.email", "test@localhost"]);
        std::fs::write(temp.0.join("staged.txt"), "included").unwrap();
        std::fs::write(temp.0.join("untracked.txt"), "retained").unwrap();
        git(&["add", "staged.txt"]);
        let hook = temp.0.join(".git/hooks/pre-commit");
        std::fs::write(&hook, "#!/bin/sh\nexit 7\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let request = GitCommitRequest {
            root: "project".into(),
            message: "test: literal $(no-shell)".into(),
            actor: "agent:test".into(),
            caps: caps("git.commit"),
            trace_id: "test".into(),
        };
        let result = execute(prepare_commit(&mgr, &request).unwrap())
            .await
            .unwrap();
        assert!(result.ok, "{}", result.stderr);
        assert!(
            String::from_utf8_lossy(&git(&["status", "--porcelain"]).stdout)
                .contains("?? untracked.txt")
        );
        assert_eq!(
            String::from_utf8_lossy(&git(&["log", "-1", "--format=%s"]).stdout).trim(),
            request.message
        );
    }
    #[test]
    fn compiler_diagnostics_map_only_workspace_paths() {
        let diagnostic = |file: &str| {
            serde_json::json!({"reason":"compiler-message","message":{"level":"error","message":"expected ;",
            "spans":[{"is_primary":true,"file_name":file,"line_start":3,"column_start":8}]}}).to_string()
        };
        let text = [
            diagnostic("src/main.rs"),
            diagnostic("../secret.rs"),
            "not JSON".into(),
        ]
        .join("\n");
        let result = parse_problems(&text, "project", Path::new("/workspace"));
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].path, "/host/project/src/main.rs");
        assert_eq!(result[0].line, 3);
    }
}
