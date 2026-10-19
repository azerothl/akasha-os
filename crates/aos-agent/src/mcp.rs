//! Client MCP JSON-RPC 2.0 sur stdio (tools/list + tools/call).

use aos_proto::McpServerInfo;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

use crate::tools::{ToolBackend, ToolDesc};

#[derive(Debug, Clone, Deserialize)]
pub struct McpServersFile {
    #[serde(default)]
    pub servers: HashMap<String, McpServerConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct McpServerConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

#[derive(Debug, Serialize)]
struct JsonRpcRequest {
    jsonrpc: &'static str,
    id: u64,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcResponse {
    result: Option<serde_json::Value>,
    error: Option<serde_json::Value>,
}

/// Session MCP stdio vers un serveur.
pub struct McpSession {
    pub name: String,
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
    group: std::sync::Arc<std::sync::Mutex<Option<ProcessGroup>>>,
    policy_watch: Option<tokio::task::JoinHandle<()>>,
}

#[cfg(windows)]
struct ProcessGroup(isize);
#[cfg(windows)]
impl ProcessGroup {
    fn new(child: &tokio::process::Child) -> Result<Self, String> {
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
struct ProcessGroup(i32);
#[cfg(unix)]
impl ProcessGroup {
    fn new(child: &tokio::process::Child) -> Result<Self, String> {
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

impl Drop for McpSession {
    fn drop(&mut self) {
        if let Some(watch) = self.policy_watch.take() {
            watch.abort();
        }
        self.group.lock().unwrap().take();
        let _ = self.child.start_kill();
    }
}

impl McpSession {
    fn watch_policy(&mut self, bus: std::sync::Arc<aos_ipc::BusClient>) {
        let group = self.group.clone();
        self.policy_watch = Some(tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                if check_host_mcp_mode(&bus).await.is_err() {
                    group.lock().unwrap().take();
                    break;
                }
            }
        }));
    }

    /// Production entry point: arbitrary host MCP servers have no network sandbox.
    pub async fn call_tool_guarded(
        &mut self,
        bus: &aos_ipc::BusClient,
        name: &str,
        args: serde_json::Value,
    ) -> Result<String, String> {
        if let Err(error) = check_host_mcp_mode(bus).await {
            self.group.lock().unwrap().take();
            let _ = self.child.start_kill();
            return Err(error);
        }
        let result = tokio::time::timeout(std::time::Duration::from_secs(60), async {
            let call = self.call_tool(name, args);
            tokio::pin!(call);
            loop {
                tokio::select! {
                    result = &mut call => break result,
                    _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {
                        check_host_mcp_mode(bus).await?;
                    }
                }
            }
        })
        .await
        .map_err(|_| "MCP tool timed out".to_string())
        .and_then(|result| result);
        if result.is_err() {
            self.group.lock().unwrap().take();
            let _ = self.child.start_kill();
        }
        result
    }
    pub async fn spawn(name: &str, cfg: &McpServerConfig) -> Result<Self, String> {
        Self::spawn_resolved(name, cfg, None).await
    }

    /// Spawn avec `env` déjà résolu (secrets interpolés en amont).
    pub async fn spawn_resolved(
        name: &str,
        cfg: &McpServerConfig,
        env_override: Option<&HashMap<String, String>>,
    ) -> Result<Self, String> {
        let mut cmd = Command::new(&cfg.command);
        cmd.args(&cfg.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let env_map = env_override.unwrap_or(&cfg.env);
        for (k, v) in env_map {
            cmd.env(k, v);
        }
        #[cfg(windows)]
        cmd.creation_flags(0x0800_0000);
        #[cfg(unix)]
        cmd.process_group(0);
        let mut child = cmd.spawn().map_err(|e| format!("spawn mcp {name}: {e}"))?;
        let group = ProcessGroup::new(&child)?;
        let stdin = child.stdin.take().ok_or("stdin mcp manquant")?;
        let stdout = child.stdout.take().ok_or("stdout mcp manquant")?;
        let mut session = Self {
            name: name.to_string(),
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: 1,
            group: std::sync::Arc::new(std::sync::Mutex::new(Some(group))),
            policy_watch: None,
        };
        let _ = session
            .request(
                "initialize",
                Some(serde_json::json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": {"name": "aos-agent", "version": env!("CARGO_PKG_VERSION")}
                })),
            )
            .await?;
        // notifications/initialized (no id)
        let notif = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });
        session
            .stdin
            .write_all(format!("{notif}\n").as_bytes())
            .await
            .map_err(|e| e.to_string())?;
        Ok(session)
    }

    async fn request(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        let req = JsonRpcRequest {
            jsonrpc: "2.0",
            id,
            method: method.to_string(),
            params,
        };
        let line = serde_json::to_string(&req).map_err(|e| e.to_string())?;
        self.stdin
            .write_all(format!("{line}\n").as_bytes())
            .await
            .map_err(|e| e.to_string())?;
        self.stdin.flush().await.map_err(|e| e.to_string())?;

        let mut buf = String::new();
        loop {
            buf.clear();
            let n = (&mut self.stdout)
                .take(1024 * 1024 + 1)
                .read_line(&mut buf)
                .await
                .map_err(|e| e.to_string())?;
            if n == 0 {
                return Err("mcp stdout fermé".into());
            }
            if n > 1024 * 1024 {
                return Err("MCP message exceeds 1 MiB".into());
            }
            let trimmed = buf.trim();
            if trimmed.is_empty() {
                continue;
            }
            // Ignore notifications
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
                if v.get("id").is_none() && v.get("method").is_some() {
                    continue;
                }
            }
            let resp: JsonRpcResponse =
                serde_json::from_str(trimmed).map_err(|e| format!("mcp parse: {e}: {trimmed}"))?;
            if let Some(err) = resp.error {
                return Err(format!("mcp error: {err}"));
            }
            return Ok(resp.result.unwrap_or(serde_json::Value::Null));
        }
    }

    pub async fn list_tools(&mut self) -> Result<Vec<ToolDesc>, String> {
        let result = self
            .request("tools/list", Some(serde_json::json!({})))
            .await?;
        let tools = result
            .get("tools")
            .and_then(|t| t.as_array())
            .cloned()
            .unwrap_or_default();
        let mut out = Vec::new();
        for t in tools {
            let name = t
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("unknown")
                .to_string();
            let description = t
                .get("description")
                .and_then(|d| d.as_str())
                .unwrap_or("")
                .to_string();
            let input_schema = t
                .get("inputSchema")
                .cloned()
                .unwrap_or(serde_json::json!({"type":"object"}));
            out.push(ToolDesc {
                name: format!("mcp.{}:{name}", self.name),
                description,
                input_schema,
                backend: ToolBackend::Mcp {
                    server: self.name.clone(),
                },
                required_caps: vec![format!("mcp.use:{}", self.name)],
            });
        }
        Ok(out)
    }

    pub async fn call_tool(
        &mut self,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<String, String> {
        // Strip mcp.<server>: prefix if present
        let short = tool_name
            .strip_prefix(&format!("mcp.{}:", self.name))
            .unwrap_or(tool_name);
        let result = self
            .request(
                "tools/call",
                Some(serde_json::json!({
                    "name": short,
                    "arguments": arguments
                })),
            )
            .await?;
        Ok(result.to_string())
    }
}

pub fn load_servers_config(path: &Path) -> McpServersFile {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_yaml::from_str(&s).ok())
        .unwrap_or(McpServersFile {
            servers: HashMap::new(),
        })
}

/// Remplace `${secret:name}` dans une valeur d'env MCP.
pub fn resolve_secret_placeholder(
    value: &str,
    secrets: &HashMap<String, String>,
) -> Result<String, String> {
    let trimmed = value.trim();
    if let Some(name) = trimmed
        .strip_prefix("${secret:")
        .and_then(|s| s.strip_suffix('}'))
    {
        return secrets
            .get(name)
            .cloned()
            .ok_or_else(|| format!("secret inconnu: {name}"));
    }
    Ok(value.to_string())
}

pub fn list_mcp_servers() -> Vec<McpServerInfo> {
    let cfg = load_servers_config(Path::new("var/mcp/servers.yaml"));
    cfg.servers
        .into_iter()
        .map(|(name, c)| McpServerInfo {
            name,
            command: c.command,
            args: c.args,
        })
        .collect()
}

/// Ouvre les sessions MCP demandées et retourne leurs outils.
/// `secrets` : carte name→valeur pour interpoler `${secret:…}` (fournie par agentd).
pub async fn open_mcp_tools(names: &[String]) -> (HashMap<String, McpSession>, Vec<ToolDesc>) {
    open_mcp_tools_with_secrets(names, &HashMap::new()).await
}

pub async fn open_mcp_tools_with_secrets(
    names: &[String],
    secrets: &HashMap<String, String>,
) -> (HashMap<String, McpSession>, Vec<ToolDesc>) {
    let cfg = load_servers_config(Path::new("var/mcp/servers.yaml"));
    let mut sessions = HashMap::new();
    let mut tools = Vec::new();
    for name in names {
        let Some(server_cfg) = cfg.servers.get(name) else {
            continue;
        };
        let mut resolved_env = HashMap::new();
        let mut env_ok = true;
        for (k, v) in &server_cfg.env {
            match resolve_secret_placeholder(v, secrets) {
                Ok(val) => {
                    resolved_env.insert(k.clone(), val);
                }
                Err(e) => {
                    eprintln!("[mcp] env {name}: {e}");
                    env_ok = false;
                    break;
                }
            }
        }
        if !env_ok {
            continue;
        }
        match McpSession::spawn_resolved(name, server_cfg, Some(&resolved_env)).await {
            Ok(mut session) => {
                match session.list_tools().await {
                    Ok(t) => tools.extend(t),
                    Err(e) => eprintln!("[mcp] tools/list {name}: {e}"),
                }
                sessions.insert(name.clone(), session);
            }
            Err(e) => eprintln!("[mcp] spawn {name}: {e}"),
        }
    }
    (sessions, tools)
}

/// Refuse startup if offline-strict or if the authoritative mode is unavailable.
/// Callers must also use call_tool_guarded to recheck before each invocation.
pub async fn open_mcp_tools_guarded(
    names: &[String],
    secrets: &HashMap<String, String>,
    bus: &std::sync::Arc<aos_ipc::BusClient>,
) -> (HashMap<String, McpSession>, Vec<ToolDesc>) {
    if names.is_empty() {
        return (HashMap::new(), Vec::new());
    }
    if let Err(error) = check_host_mcp_mode(bus).await {
        eprintln!("[mcp] {error}");
        return (HashMap::new(), Vec::new());
    }
    match tokio::time::timeout(
        std::time::Duration::from_secs(30),
        async {
            let discovery = open_mcp_tools_with_secrets(names, secrets);
            tokio::pin!(discovery);
            loop {
                tokio::select! {
                    result = &mut discovery => break result,
                    _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {
                        if check_host_mcp_mode(bus).await.is_err() { return (HashMap::new(), Vec::new()); }
                    }
                }
            }
        },
    )
    .await
    {
        Ok((mut sessions, tools)) => {
            if check_host_mcp_mode(bus).await.is_err() { return (HashMap::new(), Vec::new()); }
            for session in sessions.values_mut() {
                session.watch_policy(bus.clone());
            }
            (sessions, tools)
        },
        Err(_) => {
            eprintln!("[mcp] discovery timed out; host MCP disabled");
            (HashMap::new(), Vec::new())
        }
    }
}
pub async fn check_host_mcp_mode(bus: &aos_ipc::BusClient) -> Result<(), String> {
    let status = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        bus.call::<_, serde_json::Value>("net.status", &(), vec![]),
    )
    .await
    .map_err(|_| "MCP disabled: network policy timed out")?
    .map_err(|_| "MCP disabled: network policy unavailable")?;
    if status["mode"] != "online" {
        return Err(
            "MCP host processes are disabled in offline_strict; they are not network-sandboxed"
                .into(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn host_mcp_checks_live_mode_and_refuses_offline_or_unknown_policy() {
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let broker = tokio::spawn(async move {
            aos_ipc::broker::serve(listener).await.unwrap();
        });
        let online = Arc::new(AtomicBool::new(true));
        let mode = online.clone();
        let mut service = aos_ipc::BusService::new("mcp-network-test");
        service.on("net.status", move |ctx| {
            let mode = mode.clone();
            async move {
                let _ = ctx
                    .respond(
                        aos_ipc::msg::Status::Ok,
                        &serde_json::json!({
                "mode":if mode.load(Ordering::SeqCst){"online"}else{"offline_strict"}}),
                    )
                    .await;
            }
        });
        let service_addr = addr.clone();
        let server = tokio::spawn(async move {
            service.serve(&service_addr).await.unwrap();
        });
        let bus = aos_ipc::BusClient::connect(&addr, "test:mcp")
            .await
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            while !bus.lookup("net.status").await.unwrap() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(check_host_mcp_mode(&bus).await.is_ok());

        // Real stdio subprocess, no model or outside service. The policy watcher
        // must also reap an idle server after an online -> offline transition.
        #[cfg(windows)]
        let (command, args) = (
            "powershell.exe",
            vec![
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                r#"while ($line = [Console]::ReadLine()) { $r = ConvertFrom-Json $line; if ($null -ne $r.id) { [Console]::WriteLine((@{jsonrpc='2.0';id=$r.id;result=@{tools=@()}} | ConvertTo-Json -Depth 5 -Compress)) } }"#,
            ],
        );
        #[cfg(unix)]
        let (command, args) = (
            "/bin/sh",
            vec![
                "-c",
                r#"while IFS= read -r line; do case "$line" in *'"id":'*) printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"tools":[]}}';; esac; done"#,
            ],
        );
        let cfg = McpServerConfig {
            command: command.into(),
            args: args.into_iter().map(String::from).collect(),
            env: HashMap::new(),
        };
        let mut session = tokio::time::timeout(
            std::time::Duration::from_secs(15),
            McpSession::spawn("policy-fixture", &cfg),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(session.child.try_wait().unwrap().is_none());
        session.watch_policy(bus.clone());
        online.store(false, Ordering::SeqCst);
        assert!(check_host_mcp_mode(&bus)
            .await
            .unwrap_err()
            .contains("offline_strict"));
        let (sessions, tools) =
            open_mcp_tools_guarded(&["must-not-spawn".into()], &HashMap::new(), &bus).await;
        assert!(sessions.is_empty() && tools.is_empty());

        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            while session.child.try_wait().unwrap().is_none() {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("offline transition must stop the idle MCP process");
        assert!(session.group.lock().unwrap().is_none());
        server.abort();
        broker.abort();
    }

    #[test]
    fn resolve_secret_placeholder_ok() {
        let mut secrets = HashMap::new();
        secrets.insert("github_token".into(), "ghp_x".into());
        assert_eq!(
            resolve_secret_placeholder("${secret:github_token}", &secrets).unwrap(),
            "ghp_x"
        );
        assert_eq!(
            resolve_secret_placeholder("plain", &secrets).unwrap(),
            "plain"
        );
        assert!(resolve_secret_placeholder("${secret:missing}", &secrets).is_err());
    }
}
