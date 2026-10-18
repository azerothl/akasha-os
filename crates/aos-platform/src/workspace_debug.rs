//! Bounded DAP client for explicitly configured workspace launches (#247).
//! Protocol: https://microsoft.github.io/debug-adapter-protocol/overview.html
//! No attach, evaluate, reverse process launch, or client-supplied executable.
use crate::{workspace::WorkspaceBindManager, workspace_process::ProcessGroup};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin},
};

const FRAME_LIMIT: usize = 1024 * 1024;
pub const SESSION_SECONDS: u64 = 15 * 60;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DebugRequest {
    pub root: String,
    pub configuration: Option<String>,
    pub session_id: Option<String>,
    pub command: Option<String>,
    pub thread_id: Option<u32>,
    pub frame_id: Option<u32>,
    pub variables_reference: Option<u32>,
    pub path: Option<String>,
    #[serde(default)]
    pub lines: Vec<u32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Launch {
    adapter: Adapter,
    program: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    stop_on_entry: bool,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Adapter {
    program: PathBuf,
    #[serde(default)]
    args: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Policy {
    workspaces: HashMap<String, HashMap<String, Launch>>,
}

pub struct Prepared {
    pub workspace_id: String,
    pub root: PathBuf,
    launch: Launch,
    program: PathBuf,
}
pub fn root(mgr: &WorkspaceBindManager, raw: &str) -> Result<(String, PathBuf), String> {
    let id = raw
        .strip_prefix("/host/")
        .unwrap_or(raw)
        .trim_end_matches('/');
    if id.is_empty()
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err("debugging requires an exact bound workspace root".into());
    }
    let root = PathBuf::from(mgr.resolve_host_path(id).ok_or("workspace is not bound")?)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if !root.is_dir() {
        return Err("workspace root is not a directory".into());
    }
    Ok((id.into(), root))
}
pub fn require_caps(id: &str, caps: &[String]) -> Result<(), String> {
    for kind in ["debug.run", "fs.read", "fs.write"] {
        let cap = format!("{kind}:/host/{id}/**");
        if !caps.contains(&cap) {
            return Err(format!("required capability: {cap}"));
        }
    }
    Ok(())
}
fn source(root: &Path, relative: &str) -> Result<PathBuf, String> {
    if relative.is_empty()
        || relative.contains(['\\', ':', '\0'])
        || relative
            .split('/')
            .any(|c| c.is_empty() || c == "." || c == "..")
    {
        return Err("debug source must be a workspace-relative file".into());
    }
    let path = root
        .join(relative)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if !path.starts_with(root) || !path.is_file() {
        return Err("debug source escapes workspace".into());
    }
    Ok(path)
}
pub fn prepare(
    mgr: &WorkspaceBindManager,
    policy: &Path,
    req: &DebugRequest,
    caps: &[String],
) -> Result<Prepared, String> {
    let (id, root) = root(mgr, &req.root)?;
    require_caps(&id, caps)?;
    let policy: Policy = serde_json::from_slice(
        &std::fs::read(policy)
            .map_err(|_| "debugging is disabled: configure workspace-debug-policy.json")?,
    )
    .map_err(|e| format!("invalid debug policy: {e}"))?;
    let configuration = req
        .configuration
        .as_deref()
        .ok_or("named debug configuration required")?;
    let launch = policy
        .workspaces
        .get(&id)
        .and_then(|p| p.get(configuration))
        .ok_or("debug configuration is not allowlisted")?
        .clone();
    if !launch.adapter.program.is_absolute() || !launch.adapter.program.is_file() {
        return Err("debug adapter must be an existing absolute executable".into());
    }
    for args in [&launch.args, &launch.adapter.args] {
        if args.len() > 64 || args.iter().any(|a| a.len() > 8192 || a.contains('\0')) {
            return Err("invalid configured debug arguments".into());
        }
    }
    let program = source(&root, &launch.program)?;
    Ok(Prepared {
        workspace_id: id,
        root,
        launch,
        program,
    })
}

impl Prepared {
    pub fn description(&self) -> String {
        format!(
            "{} {:?}; programme {} {:?}",
            self.launch.adapter.program.display(),
            self.launch.adapter.args,
            self.program.display(),
            self.launch.args
        )
    }
}

struct Session {
    owner: String,
    workspace: String,
    root: PathBuf,
    _child: Child,
    _group: Option<ProcessGroup>,
    input: ChildStdin,
    output: tokio::sync::mpsc::Receiver<Result<Value, String>>,
    reader: tokio::task::JoinHandle<()>,
    seq: u64,
    replies: HashMap<u64, Value>,
    events: VecDeque<Value>,
    events_dropped: u64,
}
async fn read_frame(output: &mut (impl AsyncBufRead + Unpin)) -> Result<Value, String> {
    let mut length = None;
    let mut headers = 0;
    loop {
        let mut line = Vec::new();
        let n = (&mut *output)
            .take(8193)
            .read_until(b'\n', &mut line)
            .await
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("debug adapter closed its output".into());
        }
        headers += line.len();
        if headers > 8192 {
            return Err("debug adapter headers exceed 8 KiB".into());
        }
        if line == b"\r\n" || line == b"\n" {
            break;
        }
        let line = std::str::from_utf8(&line).map_err(|e| e.to_string())?;
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("Content-Length") {
                if length.is_some() {
                    return Err("duplicate DAP Content-Length".into());
                }
                length = Some(
                    value
                        .trim()
                        .parse::<usize>()
                        .map_err(|_| "invalid DAP Content-Length")?,
                );
            }
        }
    }
    let length = length
        .filter(|n| *n > 0 && *n <= FRAME_LIMIT)
        .ok_or("invalid DAP frame size")?;
    let mut body = vec![0; length];
    output
        .read_exact(&mut body)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::from_slice(&body).map_err(|e| e.to_string())
}

impl Session {
    async fn send(&mut self, command: &str, arguments: Value) -> Result<u64, String> {
        self.seq += 1;
        let body = serde_json::to_vec(
            &json!({"seq":self.seq,"type":"request","command":command,"arguments":arguments}),
        )
        .map_err(|e| e.to_string())?;
        self.input
            .write_all(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes())
            .await
            .map_err(|e| e.to_string())?;
        self.input
            .write_all(&body)
            .await
            .map_err(|e| e.to_string())?;
        self.input.flush().await.map_err(|e| e.to_string())?;
        Ok(self.seq)
    }
    async fn receive(&mut self) -> Result<(), String> {
        let value = self
            .output
            .recv()
            .await
            .ok_or("debug adapter output closed")??;
        self.process_message(value).await
    }
    async fn process_message(&mut self, mut value: Value) -> Result<(), String> {
        match value["type"].as_str() {
            Some("response") => {
                let id = value["request_seq"]
                    .as_u64()
                    .ok_or("DAP response has no request sequence")?;
                if self.replies.len() >= 32 {
                    return Err("too many pending DAP responses".into());
                }
                self.replies.insert(id, value);
            }
            Some("event") => {
                redact_sources(&mut value, &self.root, &self.workspace);
                if serde_json::to_vec(&value).map_err(|e| e.to_string())?.len() > 16 * 1024 {
                    value = json!({"type":"event","event":value["event"],"truncated":true});
                }
                if self.events.len() == 64 {
                    self.events.pop_front();
                    self.events_dropped += 1;
                }
                self.events.push_back(value);
            }
            Some("request") => {
                // In particular runInTerminal and startDebugging cannot launch another process.
                self.seq += 1;
                let body=serde_json::to_vec(&json!({"seq":self.seq,"type":"response","request_seq":value["seq"],
                    "command":value["command"],"success":false,"message":"reverse requests are not supported"})).unwrap();
                self.input
                    .write_all(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes())
                    .await
                    .map_err(|e| e.to_string())?;
                self.input
                    .write_all(&body)
                    .await
                    .map_err(|e| e.to_string())?;
                self.input.flush().await.map_err(|e| e.to_string())?;
            }
            _ => return Err("invalid DAP message type".into()),
        }
        Ok(())
    }
    async fn response(&mut self, seq: u64) -> Result<Value, String> {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if let Some(response) = self.replies.remove(&seq) {
                    if response["success"] != true {
                        return Err(response["message"]
                            .as_str()
                            .unwrap_or("debug adapter refused request")
                            .into());
                    }
                    return Ok(response["body"].clone());
                }
                self.receive().await?;
            }
        })
        .await
        .map_err(|_| "debug adapter response timed out".to_string())?
    }
    async fn rpc(&mut self, command: &str, args: Value) -> Result<Value, String> {
        let seq = self.send(command, args).await?;
        self.response(seq).await
    }
    async fn event(&mut self, name: &str) -> Result<(), String> {
        tokio::time::timeout(Duration::from_secs(10), async {
            while !self.events.iter().any(|e| e["event"] == name) {
                self.receive().await?;
            }
            Ok(())
        })
        .await
        .map_err(|_| format!("DAP event timed out: {name}"))?
    }
    async fn start(prepared: Prepared, owner: &str) -> Result<Self, String> {
        let mut cmd = tokio::process::Command::new(&prepared.launch.adapter.program);
        cmd.args(&prepared.launch.adapter.args)
            .current_dir(&prepared.root)
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        for name in [
            "PATH",
            "SystemRoot",
            "WINDIR",
            "TEMP",
            "TMP",
            "HOME",
            "USERPROFILE",
            "LOCALAPPDATA",
            "APPDATA",
        ] {
            if let Some(value) = std::env::var_os(name) {
                cmd.env(name, value);
            }
        }
        #[cfg(windows)]
        cmd.creation_flags(0x0800_0000);
        #[cfg(unix)]
        cmd.process_group(0);
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("debug adapter launch failed: {e}"))?;
        let group = ProcessGroup::new(&child)?;
        let input = child.stdin.take().ok_or("DAP stdin unavailable")?;
        let mut output = BufReader::new(child.stdout.take().ok_or("DAP stdout unavailable")?);
        let (sender, output_rx) = tokio::sync::mpsc::channel(8);
        let reader = tokio::spawn(async move {
            loop {
                let result = read_frame(&mut output).await;
                let failed = result.is_err();
                if sender.send(result).await.is_err() || failed {
                    break;
                }
            }
        });
        let mut session = Self {
            owner: owner.into(),
            workspace: prepared.workspace_id,
            root: prepared.root.clone(),
            _child: child,
            _group: Some(group),
            input,
            output: output_rx,
            reader,
            seq: 0,
            replies: HashMap::new(),
            events: VecDeque::new(),
            events_dropped: 0,
        };
        let capabilities=session.rpc("initialize",json!({"clientID":"akasha-os","adapterID":"akasha",
            "pathFormat":"path","linesStartAt1":true,"columnsStartAt1":true,"supportsRunInTerminalRequest":false,
            "supportsVariablePaging":true,"supportsStartDebuggingRequest":false})).await?;
        let launch=session.send("launch",json!({"program":prepared.program,"cwd":prepared.root,"args":prepared.launch.args,
            "console":"internalConsole","justMyCode":true,"stopOnEntry":prepared.launch.stop_on_entry})).await?;
        session.event("initialized").await?;
        if capabilities["supportsConfigurationDoneRequest"] == true {
            session.rpc("configurationDone", json!({})).await?;
        } else {
            session
                .rpc("setExceptionBreakpoints", json!({"filters":[]}))
                .await?;
        }
        session.response(launch).await?;
        Ok(session)
    }
    async fn shutdown(mut self) {
        let _ = tokio::time::timeout(
            Duration::from_secs(1),
            self.rpc("disconnect", json!({"terminateDebuggee":true})),
        )
        .await;
        self.reader.abort();
        drop(self._group.take());
        let _ = self._child.start_kill();
        let _ = tokio::time::timeout(Duration::from_secs(2), self._child.wait()).await;
    }
    fn take_events(&mut self) -> Value {
        json!({"events":self.events.drain(..).collect::<Vec<_>>(),"events_dropped":self.events_dropped})
    }
    async fn request(&mut self, req: &DebugRequest) -> Result<Value, String> {
        let command = req.command.as_deref().ok_or("debug command required")?;
        let thread = || req.thread_id.filter(|n| *n > 0).ok_or("thread_id required");
        let (command, args) = match command {
            "threads" => ("threads", json!({})),
            "stackTrace" => (
                "stackTrace",
                json!({"threadId":thread()?,"startFrame":0,"levels":50}),
            ),
            "scopes" => (
                "scopes",
                json!({"frameId":req.frame_id.ok_or("frame_id required")?}),
            ),
            "variables" => (
                "variables",
                json!({"variablesReference":req.variables_reference.filter(|n|*n>0)
                .ok_or("variables_reference required")?,"start":0,"count":100}),
            ),
            "continue" | "next" | "stepIn" | "stepOut" | "pause" => {
                (command, json!({"threadId":thread()?}))
            }
            "setBreakpoints" => {
                if req.lines.len() > 64 || req.lines.iter().any(|n| *n == 0 || *n > 1_000_000) {
                    return Err("invalid debug breakpoints".into());
                }
                let path = source(
                    &self.root,
                    req.path.as_deref().ok_or("breakpoint path required")?,
                )?;
                (
                    "setBreakpoints",
                    json!({"source":{"path":path},"breakpoints":req.lines.iter().map(|line|json!({"line":line})).collect::<Vec<_>>()}),
                )
            }
            "events" => {
                if self.events.is_empty() {
                    if let Ok(message) =
                        tokio::time::timeout(Duration::from_millis(250), self.output.recv()).await
                    {
                        self.process_message(message.ok_or("debug adapter output closed")??)
                            .await?;
                    }
                }
                return Ok(self.take_events());
            }
            _ => return Err("debug command is not allowed".into()),
        };
        let mut body = self.rpc(command, args).await?;
        redact_sources(&mut body, &self.root, &self.workspace);
        Ok(json!({"body":body,"activity":self.take_events()}))
    }
}

fn redact_sources(value: &mut Value, root: &Path, id: &str) {
    match value {
        Value::Object(fields) => {
            for (key, value) in fields.iter_mut() {
                if key == "path" {
                    if let Some(raw) = value.as_str() {
                        *value = Path::new(raw)
                            .canonicalize()
                            .ok()
                            .and_then(|p| p.strip_prefix(root).ok().map(Path::to_path_buf))
                            .map(|p| {
                                Value::String(format!(
                                    "/host/{id}/{}",
                                    p.to_string_lossy().replace('\\', "/")
                                ))
                            })
                            .unwrap_or(Value::Null);
                    }
                } else {
                    redact_sources(value, root, id);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                redact_sources(item, root, id);
            }
        }
        _ => {}
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.reader.abort();
    }
}

#[derive(Default)]
pub struct DebugManager {
    sessions: HashMap<String, Session>,
    next: u64,
}

impl DebugManager {
    pub async fn start(&mut self, prepared: Prepared, actor: &str) -> Result<Value, String> {
        if self.sessions.len() >= 4 {
            return Err("debug session limit reached (4)".into());
        }
        let workspace = prepared.workspace_id.clone();
        let session =
            tokio::time::timeout(Duration::from_secs(45), Session::start(prepared, actor))
                .await
                .map_err(|_| "debug startup timed out")??;
        self.next += 1;
        let id = format!("debug-{}-{}", std::process::id(), self.next);
        self.sessions.insert(id.clone(), session);
        Ok(
            json!({"ok":true,"session_id":id,"workspace_id":workspace,"expires_after_sec":SESSION_SECONDS}),
        )
    }
    pub async fn request(
        &mut self,
        id: &str,
        actor: &str,
        workspace: &str,
        req: &DebugRequest,
    ) -> Result<Value, String> {
        let session = self
            .sessions
            .get_mut(id)
            .ok_or("debug session unavailable")?;
        if session.owner != actor || session.workspace != workspace {
            return Err("debug session belongs to a different actor/workspace".into());
        }
        let result = tokio::time::timeout(Duration::from_secs(15), session.request(req))
            .await
            .map_err(|_| "debug request timed out".to_string())
            .and_then(|r| r);
        if result.is_err() {
            self.sessions.remove(id);
        }
        result
    }
    pub async fn stop(&mut self, id: &str, actor: &str, workspace: &str) -> Result<Value, String> {
        let session = self.sessions.get(id).ok_or("debug session unavailable")?;
        if session.owner != actor || session.workspace != workspace {
            return Err("debug session belongs to a different actor/workspace".into());
        }
        if let Some(session) = self.sessions.remove(id) {
            session.shutdown().await;
        }
        Ok(json!({"ok":true,"session_id":id,"stopped":true}))
    }
    pub async fn stop_all(&mut self) {
        let sessions = std::mem::take(&mut self.sessions);
        for (_, session) in sessions {
            session.shutdown().await;
        }
    }
    pub fn expire(&mut self, id: &str) {
        self.sessions.remove(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn framed_messages_survive_fragmented_transport_and_idle_polls() {
        let (mut writer, reader) = tokio::io::duplex(128);
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let pump = tokio::spawn(async move {
            let result = read_frame(&mut BufReader::new(reader)).await;
            tx.send(result).await.unwrap();
        });
        writer.write_all(b"Content-Len").await.unwrap();
        assert!(tokio::time::timeout(Duration::from_millis(10), rx.recv())
            .await
            .is_err());
        let body = json!({"type":"event","event":"stopped"}).to_string();
        writer
            .write_all(format!("gth: {}\r\n\r\n{}", body.len(), body).as_bytes())
            .await
            .unwrap();
        assert_eq!(rx.recv().await.unwrap().unwrap()["event"], "stopped");
        pump.await.unwrap();
    }
    #[tokio::test]
    async fn oversized_and_ambiguous_frames_fail_before_body_allocation() {
        for header in [
            "Content-Length: 1048577\r\n\r\n",
            "Content-Length: 2\r\nContent-Length: 2\r\n\r\n",
            "Content-Length: -1\r\n\r\n",
            "Content-Length: 0\r\n\r\n",
        ] {
            let result = read_frame(&mut BufReader::new(header.as_bytes())).await;
            assert!(result.is_err(), "{header}");
        }
    }
    #[test]
    fn source_scope_and_debug_grants_are_explicit() {
        let root = std::env::temp_dir().canonicalize().unwrap();
        for name in [
            "../outside",
            "/absolute",
            "C:/absolute",
            "dir\\outside",
            "./file",
            "dir/../outside",
        ] {
            assert!(source(&root, name).is_err());
        }
        assert!(require_caps("project", &[]).is_err());
        assert!(require_caps(
            "project",
            &[
                "debug.run:/host/other/**".into(),
                "fs.read:/host/project/**".into(),
                "fs.write:/host/project/**".into()
            ]
        )
        .is_err());
        assert!(require_caps(
            "project",
            &[
                "debug.run:/host/project/**".into(),
                "fs.read:/host/project/**".into(),
                "fs.write:/host/project/**".into()
            ]
        )
        .is_ok());
        assert!(
            serde_json::from_value::<DebugRequest>(json!({"root":"project","program":"evil"}))
                .is_err()
        );
    }
    #[test]
    fn debugger_sources_never_expose_ungranted_absolute_paths() {
        let root = std::env::temp_dir().canonicalize().unwrap();
        let mut message = json!({"source":{"path":"C:/outside/workspace.py"}});
        redact_sources(&mut message, &root, "project");
        assert!(message["source"]["path"].is_null());
    }
}
