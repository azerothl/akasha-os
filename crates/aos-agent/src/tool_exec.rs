//! Tool invocation for lightweight runtimes (room turns).

use aos_ipc::BusClient;
use aos_proto::{
    FsListRequest, FsReadRequest, FsReadResponse, FsWriteRequest, ModuleInvokeRequest,
    ModuleInvokeResponse, WebBrowseRequest, WebBrowseResponse, WebSearchRequest, WebSearchResponse,
};
use crate::mcp::McpSession;
use crate::tools::{
    canonicalize_tool_name, is_module_fallback_candidate, normalize_tool_args, resolve_tool_backend,
    ToolBackend, ToolDesc,
};
use std::collections::HashMap;

/// Format a module.invoke result for agent tool_result (unwrap JSON strings).
pub fn format_module_invoke_result(result: &serde_json::Value) -> String {
    match result {
        serde_json::Value::String(s) => s.clone(),
        _ => result.to_string(),
    }
}

/// Invoke a WASM module tool (`notes.*`, `tasks.*`, `canvas.*`, …).
pub async fn invoke_module_tool(
    bus: &BusClient,
    agent_id: &str,
    caps: &[String],
    tool: &str,
    args: &serde_json::Value,
    trace_id: &str,
    session_id: Option<&str>,
) -> String {
    let module = tool.split('.').next().unwrap_or("").to_string();
    let mut args = args.clone();
    if module == "canvas" {
        let sid = match session_id.filter(|s| !s.is_empty()) {
            Some(s) => s,
            None => return "ERREUR outil: canvas.* requiert un session_id lié".to_string(),
        };
        let orig = args.clone();
        args = serde_json::json!({});
        if let Some(obj) = args.as_object_mut() {
            if let Some(orig_obj) = orig.as_object() {
                for (k, v) in orig_obj {
                    obj.insert(k.clone(), v.clone());
                }
            }
            obj.insert("session_id".into(), serde_json::json!(sid));
            obj.insert("author_id".into(), serde_json::json!(agent_id));
        }
    }
    let req = ModuleInvokeRequest {
        module,
        tool: tool.to_string(),
        args,
        actor: format!("agent:{agent_id}"),
        actor_caps: caps.to_vec(),
        trace_id: trace_id.to_string(),
    };
    match bus
        .call::<ModuleInvokeRequest, ModuleInvokeResponse>("module.invoke", &req, vec![])
        .await
    {
        Ok(resp) if resp.ok => format_module_invoke_result(&resp.result),
        Ok(resp) => format!("ERREUR outil: {}", resp.error.unwrap_or_default()),
        Err(e) => format!("ERREUR bus: {e}"),
    }
}

async fn read_fs(bus: &BusClient, path: &str, agent_id: &str, caps: &[String]) -> String {
    match bus
        .call::<FsReadRequest, FsReadResponse>(
            "fs.read",
            &FsReadRequest {
                path: path.to_string(),
                actor: format!("agent:{agent_id}"),
                caps: caps.to_vec(),
            },
            vec![],
        )
        .await
    {
        Ok(r) => r.content,
        Err(e) => format!("fs.read err: {e}"),
    }
}

/// Invoke a native platform tool (subset used by room members).
pub async fn invoke_native_tool(
    bus: &BusClient,
    agent_id: &str,
    caps: &[String],
    tool: &str,
    args: &serde_json::Value,
) -> String {
    let actor = format!("agent:{agent_id}");
    match tool {
        "fs.read" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            read_fs(bus, path, agent_id, caps).await
        }
        "fs.write" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let content = args
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            match bus
                .call::<FsWriteRequest, serde_json::Value>(
                    "fs.write",
                    &FsWriteRequest {
                        path: path.clone(),
                        content,
                        tx_id: None,
                        actor,
                        caps: caps.to_vec(),
                        trace_id: String::new(),
                    },
                    vec![],
                )
                .await
            {
                Ok(_) => format!("écrit {path}"),
                Err(e) => format!("fs.write err: {e}"),
            }
        }
        "fs.list" => {
            let prefix = args
                .get("prefix")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            match bus
                .call::<FsListRequest, Vec<aos_proto::FsEntry>>(
                    "fs.list",
                    &FsListRequest {
                        prefix,
                        caps: caps.to_vec(),
                    },
                    vec![],
                )
                .await
            {
                Ok(entries) => serde_json::to_string(&entries).unwrap_or_default(),
                Err(e) => format!("fs.list err: {e}"),
            }
        }
        "web.search" => {
            let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let engine = args
                .get("engine")
                .and_then(|v| v.as_str())
                .unwrap_or("auto")
                .to_string();
            match bus
                .call::<WebSearchRequest, WebSearchResponse>(
                    "web.search",
                    &WebSearchRequest {
                        query,
                        max_results: args
                            .get("max_results")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(5) as usize,
                        caps: caps.to_vec(),
                        actor,
                        engine,
                    },
                    vec![],
                )
                .await
            {
                Ok(r) => serde_json::to_string(&r.results).unwrap_or_default(),
                Err(e) => format!("web.search err: {e}"),
            }
        }
        "web.browse" => {
            let url = args.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string();
            match bus
                .call::<WebBrowseRequest, WebBrowseResponse>(
                    "web.browse",
                    &WebBrowseRequest {
                        url,
                        max_chars: args
                            .get("max_chars")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(12_000) as usize,
                        caps: caps.to_vec(),
                        actor,
                    },
                    vec![],
                )
                .await
            {
                Ok(r) => serde_json::to_string(&r).unwrap_or_default(),
                Err(e) => format!("web.browse err: {e}"),
            }
        }
        other => format!("outil natif non supporté en salon: {other}"),
    }
}

/// Execute one room-member tool action.
#[allow(clippy::too_many_arguments)] // Keeps capability and MCP context explicit at the tool boundary.
pub async fn execute_room_tool(
    bus: &BusClient,
    agent_id: &str,
    caps: &[String],
    tools: &[ToolDesc],
    action: &str,
    args: &serde_json::Value,
    trace_id: &str,
    session_id: Option<&str>,
    mcp_sessions: &mut HashMap<String, McpSession>,
) -> String {
    let canonical = canonicalize_tool_name(action);
    let name = canonical.as_str();
    let args_owned = normalize_tool_args(name, args);
    let args = &args_owned;

    if matches!(name, "agent.spawn" | "agent.await" | "user.ask" | "goal.complete" | "goal.fail")
    {
        return format!("action {name} indisponible en tour de salon — réponds en texte");
    }

    let backend = resolve_tool_backend(name, tools);
    match backend {
        Some(ToolBackend::Module) => {
            invoke_module_tool(bus, agent_id, caps, name, args, trace_id, session_id).await
        }
        None if is_module_fallback_candidate(name) => {
            invoke_module_tool(bus, agent_id, caps, name, args, trace_id, session_id).await
        }
        Some(ToolBackend::Native) => invoke_native_tool(bus, agent_id, caps, name, args).await,
        Some(ToolBackend::Mcp { server }) => {
            if let Some(session) = mcp_sessions.get_mut(&server) {
                match session.call_tool(name, args.clone()).await {
                    Ok(r) => r,
                    Err(e) => format!("mcp err: {e}"),
                }
            } else {
                format!("session mcp {server} absente")
            }
        }
        Some(ToolBackend::Runtime) => format!("action runtime indisponible en salon: {name}"),
        None if name.starts_with("mcp.") => {
            if let Some(rest) = name.strip_prefix("mcp.") {
                let server = rest.split(':').next().unwrap_or("");
                if let Some(session) = mcp_sessions.get_mut(server) {
                    match session.call_tool(name, args.clone()).await {
                        Ok(r) => r,
                        Err(e) => format!("mcp err: {e}"),
                    }
                } else {
                    format!("mcp server {server} non ouvert")
                }
            } else {
                format!("nom mcp invalide: {name}")
            }
        }
        None => format!("outil inconnu en salon: {name}"),
    }
}

#[cfg(test)]
mod tests {
    use super::format_module_invoke_result;

    #[test]
    fn format_module_invoke_result_unwraps_json_string() {
        let v = serde_json::json!("ok seq=12 ellipse bbox=(0.350,0.150)-(0.650,0.270)");
        assert_eq!(
            format_module_invoke_result(&v),
            "ok seq=12 ellipse bbox=(0.350,0.150)-(0.650,0.270)"
        );
    }
}
