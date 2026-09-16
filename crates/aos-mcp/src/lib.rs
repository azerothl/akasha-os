//! Optional stdio MCP façade: external IDEs → Akasha bus (`model.*` + `mem.*`).
//!
//! Identity defaults to `service:mcp`. No secrets, shell, or agent control in MVP.

use aos_ipc::client::{BusClient, CallError};
use aos_proto::{
    ChatMessage, InferParams, InferRequest, MemContextRequest, MemListRequest, MemStats,
    MemUserRecallRequest, MemUserRememberRequest, ModelInfo, TokenEvent,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;

pub const PROTOCOL_VERSION: &str = "2024-11-05";
pub const DEFAULT_FROM: &str = "service:mcp";
pub const DEFAULT_BUS: &str = "127.0.0.1:24701";
pub const SERVER_NAME: &str = "aos-mcpd";

#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: Option<String>,
    pub id: Option<Value>,
    pub method: Option<String>,
    #[serde(default)]
    pub params: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcResponse {
    pub fn result(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: Option<Value>, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }
}

/// Tool catalogue for `tools/list` (stable names for IDE clients).
pub fn tool_catalog() -> Value {
    json!({
        "tools": [
            {
                "name": "akasha_models",
                "description": "List local / provider models known to Akasha (`model.list`).",
                "inputSchema": {
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }
            },
            {
                "name": "akasha_infer",
                "description": "Run a buffered chat completion via Akasha (`model.infer`). Returns concatenated text plus Done metrics.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "messages": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "role": { "type": "string" },
                                    "content": { "type": "string" }
                                },
                                "required": ["role", "content"]
                            }
                        },
                        "prompt": {
                            "type": "string",
                            "description": "Shortcut for a single user message when `messages` is omitted."
                        },
                        "model_id": { "type": ["string", "null"] },
                        "max_tokens": { "type": "integer", "minimum": 1 },
                        "temperature": { "type": "number" },
                        "top_p": { "type": "number" },
                        "routing": { "type": ["string", "null"] }
                    },
                    "additionalProperties": false
                }
            },
            {
                "name": "akasha_mem_stats",
                "description": "Memory store stats (`mem.stats`).",
                "inputSchema": {
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }
            },
            {
                "name": "akasha_mem_context",
                "description": "Retrieve grounded memory context for a query (`mem.context`).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" },
                        "session_id": { "type": ["string", "null"] },
                        "namespace": { "type": ["string", "null"] },
                        "k": { "type": "integer", "minimum": 1 },
                        "product_k": { "type": "integer", "minimum": 0 },
                        "user_doc_k": { "type": "integer", "minimum": 0 }
                    },
                    "required": ["query"],
                    "additionalProperties": false
                }
            },
            {
                "name": "akasha_mem_list",
                "description": "List memory entries in a namespace (`mem.list`).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "namespace": { "type": "string" },
                        "include_superseded": { "type": "boolean" }
                    },
                    "required": ["namespace"],
                    "additionalProperties": false
                }
            },
            {
                "name": "akasha_mem_recall",
                "description": "Semantic recall over user facts (`mem.user.recall`).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" },
                        "k": { "type": "integer", "minimum": 1 }
                    },
                    "required": ["query"],
                    "additionalProperties": false
                }
            },
            {
                "name": "akasha_mem_remember",
                "description": "Store a user fact (`mem.user.remember`).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "text": { "type": "string" },
                        "pinned": { "type": "boolean" },
                        "auto_link": { "type": "boolean" }
                    },
                    "required": ["text"],
                    "additionalProperties": false
                }
            }
        ]
    })
}

pub fn initialize_result() -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": {
            "tools": {}
        },
        "serverInfo": {
            "name": SERVER_NAME,
            "version": env!("CARGO_PKG_VERSION")
        }
    })
}

fn tool_text_result(text: impl Into<String>, is_error: bool) -> Value {
    json!({
        "content": [{ "type": "text", "text": text.into() }],
        "isError": is_error
    })
}

fn tool_json_result(value: &Value) -> Value {
    tool_text_result(
        serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string()),
        false,
    )
}

fn arg_str(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn arg_usize(args: &Value, key: &str) -> Option<usize> {
    args.get(key).and_then(|v| {
        v.as_u64()
            .map(|n| n as usize)
            .or_else(|| v.as_i64().filter(|n| *n >= 0).map(|n| n as usize))
    })
}

fn arg_f32(args: &Value, key: &str, default: f32) -> f32 {
    args.get(key)
        .and_then(|v| v.as_f64())
        .map(|n| n as f32)
        .unwrap_or(default)
}

fn arg_u32(args: &Value, key: &str, default: u32) -> u32 {
    args.get(key)
        .and_then(|v| v.as_u64())
        .map(|n| n as u32)
        .unwrap_or(default)
}

fn arg_bool(args: &Value, key: &str, default: bool) -> bool {
    args.get(key).and_then(|v| v.as_bool()).unwrap_or(default)
}

fn call_error_text(err: CallError) -> String {
    match err {
        CallError::Status { status, message } => format!("{status:?}: {message}"),
        other => other.to_string(),
    }
}

/// Shared bus connection for the MCP process lifetime.
pub struct BusState {
    bus_addr: String,
    from: String,
    client: Mutex<Option<Arc<BusClient>>>,
}

impl BusState {
    pub fn new(bus_addr: String, from: String) -> Self {
        Self {
            bus_addr,
            from,
            client: Mutex::new(None),
        }
    }

    pub fn from_env() -> Self {
        let bus_addr = std::env::var("AOS_BUS_ADDR").unwrap_or_else(|_| DEFAULT_BUS.into());
        let from = std::env::var("AOS_MCP_FROM").unwrap_or_else(|_| DEFAULT_FROM.into());
        Self::new(bus_addr, from)
    }

    pub fn bus_addr(&self) -> &str {
        &self.bus_addr
    }

    pub fn from(&self) -> &str {
        &self.from
    }

    async fn client(&self) -> Result<Arc<BusClient>, CallError> {
        let mut guard = self.client.lock().await;
        if let Some(c) = guard.as_ref() {
            return Ok(c.clone());
        }
        let c = BusClient::connect(&self.bus_addr, &self.from).await?;
        *guard = Some(c.clone());
        Ok(c)
    }

    pub async fn call_tool(&self, name: &str, args: Value) -> Value {
        match self.call_tool_inner(name, args).await {
            Ok(v) => v,
            Err(msg) => tool_text_result(msg, true),
        }
    }

    async fn call_tool_inner(&self, name: &str, args: Value) -> Result<Value, String> {
        let client = self
            .client()
            .await
            .map_err(|e| format!("bus connect ({}): {e}", self.bus_addr))?;

        match name {
            "akasha_models" => {
                let models: Vec<ModelInfo> = client
                    .call("model.list", &(), vec![])
                    .await
                    .map_err(call_error_text)?;
                Ok(tool_json_result(&serde_json::to_value(models).map_err(|e| e.to_string())?))
            }
            "akasha_infer" => self.infer(&client, &args).await,
            "akasha_mem_stats" => {
                let stats: MemStats = client
                    .call("mem.stats", &(), vec![])
                    .await
                    .map_err(call_error_text)?;
                Ok(tool_json_result(&serde_json::to_value(stats).map_err(|e| e.to_string())?))
            }
            "akasha_mem_context" => {
                let query = arg_str(&args, "query").ok_or_else(|| "missing query".to_string())?;
                let req = MemContextRequest {
                    session_id: arg_str(&args, "session_id"),
                    namespace: arg_str(&args, "namespace"),
                    query,
                    k: arg_usize(&args, "k").unwrap_or(5),
                    product_k: arg_usize(&args, "product_k").unwrap_or(0),
                    user_doc_k: arg_usize(&args, "user_doc_k").unwrap_or(0),
                };
                let resp: Value = client
                    .call("mem.context", &req, vec![])
                    .await
                    .map_err(call_error_text)?;
                Ok(tool_json_result(&resp))
            }
            "akasha_mem_list" => {
                let namespace =
                    arg_str(&args, "namespace").ok_or_else(|| "missing namespace".to_string())?;
                let req = MemListRequest {
                    namespace,
                    include_superseded: arg_bool(&args, "include_superseded", false),
                };
                let resp: Value = client
                    .call("mem.list", &req, vec![])
                    .await
                    .map_err(call_error_text)?;
                Ok(tool_json_result(&resp))
            }
            "akasha_mem_recall" => {
                let query = arg_str(&args, "query").ok_or_else(|| "missing query".to_string())?;
                let req = MemUserRecallRequest {
                    query,
                    k: arg_usize(&args, "k").unwrap_or(5),
                };
                let resp: Value = client
                    .call("mem.user.recall", &req, vec![])
                    .await
                    .map_err(call_error_text)?;
                Ok(tool_json_result(&resp))
            }
            "akasha_mem_remember" => {
                let text = arg_str(&args, "text").ok_or_else(|| "missing text".to_string())?;
                let req = MemUserRememberRequest {
                    text,
                    metadata: Value::Null,
                    pinned: arg_bool(&args, "pinned", false),
                    auto_link: arg_bool(&args, "auto_link", true),
                    ..MemUserRememberRequest::default()
                };
                let resp: Value = client
                    .call("mem.user.remember", &req, vec![])
                    .await
                    .map_err(call_error_text)?;
                Ok(tool_json_result(&resp))
            }
            other => Err(format!("unknown tool: {other}")),
        }
    }

    async fn infer(&self, client: &BusClient, args: &Value) -> Result<Value, String> {
        let messages = if let Some(arr) = args.get("messages").and_then(|v| v.as_array()) {
            let mut out = Vec::with_capacity(arr.len());
            for m in arr {
                let role = m
                    .get("role")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "message.role required".to_string())?
                    .to_string();
                let content = m
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "message.content required".to_string())?
                    .to_string();
                out.push(ChatMessage { role, content });
            }
            out
        } else if let Some(prompt) = arg_str(args, "prompt") {
            vec![ChatMessage {
                role: "user".into(),
                content: prompt,
            }]
        } else {
            return Err("akasha_infer requires `messages` or `prompt`".into());
        };

        if messages.is_empty() {
            return Err("akasha_infer: empty messages".into());
        }

        let req = InferRequest {
            model_id: arg_str(args, "model_id"),
            messages,
            tools: Vec::new(),
            params: InferParams {
                max_tokens: arg_u32(args, "max_tokens", 256),
                temperature: arg_f32(args, "temperature", 0.7),
                top_p: arg_f32(args, "top_p", 0.9),
                seed: None,
            },
            priority: 2,
            data_refs: Vec::new(),
            images: Vec::new(),
            routing: arg_str(args, "routing"),
        };

        let mut rx = client
            .call_stream::<InferRequest, TokenEvent>("model.infer", &req, vec![])
            .await
            .map_err(call_error_text)?;

        let mut text = String::new();
        let mut metrics: Option<Value> = None;
        let mut err_msg: Option<String> = None;

        while let Some(item) = rx.recv().await {
            match item.map_err(call_error_text)? {
                TokenEvent::Delta { text: delta } => text.push_str(&delta),
                TokenEvent::Done {
                    prompt_tokens,
                    generated_tokens,
                    ttft_ms,
                    tok_s,
                } => {
                    metrics = Some(json!({
                        "prompt_tokens": prompt_tokens,
                        "generated_tokens": generated_tokens,
                        "ttft_ms": ttft_ms,
                        "tok_s": tok_s,
                    }));
                }
                TokenEvent::Error { message } => {
                    err_msg = Some(message);
                    break;
                }
                TokenEvent::Started { .. } | TokenEvent::Queued { .. } => {}
            }
        }

        if let Some(message) = err_msg {
            return Ok(tool_text_result(format!("infer error: {message}"), true));
        }

        Ok(tool_json_result(&json!({
            "text": text,
            "metrics": metrics,
        })))
    }
}

/// Handle one JSON-RPC request/notification. Returns `None` for notifications.
pub async fn handle_message(bus: &BusState, req: JsonRpcRequest) -> Option<JsonRpcResponse> {
    let id = req.id.clone();
    let method = match req.method.as_deref() {
        Some(m) => m,
        None => {
            return Some(JsonRpcResponse::error(id, -32600, "missing method"));
        }
    };

    // Notifications have no id — no response.
    let is_notification = id.is_none();

    match method {
        "notifications/initialized" | "notifications/cancelled" => None,
        "initialize" => Some(JsonRpcResponse::result(id, initialize_result())),
        "ping" => Some(JsonRpcResponse::result(id, json!({}))),
        "tools/list" => Some(JsonRpcResponse::result(id, tool_catalog())),
        "tools/call" => {
            if is_notification {
                return None;
            }
            let params = req.params.unwrap_or_else(|| json!({}));
            let name = params
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if name.is_empty() {
                return Some(JsonRpcResponse::error(id, -32602, "tools/call missing name"));
            }
            let args = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let result = bus.call_tool(&name, args).await;
            Some(JsonRpcResponse::result(id, result))
        }
        other if is_notification => {
            tracing::debug!("ignored notification: {other}");
            None
        }
        other => Some(JsonRpcResponse::error(
            id,
            -32601,
            format!("method not found: {other}"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_exposes_mvp_tools() {
        let tools = tool_catalog()
            .get("tools")
            .and_then(|t| t.as_array())
            .cloned()
            .unwrap_or_default();
        let names: Vec<&str> = tools
            .iter()
            .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
            .collect();
        assert!(names.contains(&"akasha_models"));
        assert!(names.contains(&"akasha_infer"));
        assert!(names.contains(&"akasha_mem_stats"));
        assert!(names.contains(&"akasha_mem_context"));
        assert!(names.contains(&"akasha_mem_list"));
        assert!(names.contains(&"akasha_mem_recall"));
        assert!(names.contains(&"akasha_mem_remember"));
        assert_eq!(names.len(), 7);
    }

    #[tokio::test]
    async fn initialize_and_list_need_no_bus() {
        let bus = BusState::new("127.0.0.1:1".into(), "service:mcp".into());
        let init = handle_message(
            &bus,
            JsonRpcRequest {
                jsonrpc: Some("2.0".into()),
                id: Some(json!(1)),
                method: Some("initialize".into()),
                params: Some(json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": {"name": "test", "version": "0"}
                })),
            },
        )
        .await
        .expect("initialize response");
        assert!(init.error.is_none());
        let result = init.result.expect("result");
        assert_eq!(
            result.get("protocolVersion").and_then(|v| v.as_str()),
            Some(PROTOCOL_VERSION)
        );

        let list = handle_message(
            &bus,
            JsonRpcRequest {
                jsonrpc: Some("2.0".into()),
                id: Some(json!(2)),
                method: Some("tools/list".into()),
                params: Some(json!({})),
            },
        )
        .await
        .expect("tools/list response");
        assert!(list.error.is_none());
        let tools = list
            .result
            .as_ref()
            .and_then(|r| r.get("tools"))
            .and_then(|t| t.as_array())
            .expect("tools array");
        assert_eq!(tools.len(), 7);
    }

    #[tokio::test]
    async fn initialized_notification_is_silent() {
        let bus = BusState::new("127.0.0.1:1".into(), "service:mcp".into());
        let resp = handle_message(
            &bus,
            JsonRpcRequest {
                jsonrpc: Some("2.0".into()),
                id: None,
                method: Some("notifications/initialized".into()),
                params: None,
            },
        )
        .await;
        assert!(resp.is_none());
    }

    #[test]
    fn response_json_has_no_embedded_newlines() {
        let resp = JsonRpcResponse::result(Some(json!(1)), initialize_result());
        let line = serde_json::to_string(&resp).unwrap();
        assert!(!line.contains('\n'));
    }
}
