//! Client MCP JSON-RPC 2.0 sur stdio (tools/list + tools/call).

use aos_proto::McpServerInfo;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
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
    #[allow(dead_code)]
    jsonrpc: Option<String>,
    #[allow(dead_code)]
    id: Option<u64>,
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
}

impl McpSession {
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
        let mut child = cmd.spawn().map_err(|e| format!("spawn mcp {name}: {e}"))?;
        let stdin = child.stdin.take().ok_or("stdin mcp manquant")?;
        let stdout = child.stdout.take().ok_or("stdout mcp manquant")?;
        let mut session = Self {
            name: name.to_string(),
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: 1,
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
            let n = self
                .stdout
                .read_line(&mut buf)
                .await
                .map_err(|e| e.to_string())?;
            if n == 0 {
                return Err("mcp stdout fermé".into());
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
        let result = self.request("tools/list", Some(serde_json::json!({}))).await?;
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

impl Drop for McpSession {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
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
pub async fn open_mcp_tools(
    names: &[String],
) -> (HashMap<String, McpSession>, Vec<ToolDesc>) {
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

#[cfg(test)]
mod tests {
    use super::*;

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
