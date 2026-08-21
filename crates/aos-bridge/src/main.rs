//! Optional loopback HTTP ↔ Semantic IPC Bus adapter (E8 live).
//!
//! Bind is always `127.0.0.1`. Identity comes from `X-Aos-From` (default
//! `service:bridge`). Not spawned by `aos-session`.

use aos_ipc::client::{BusClient, CallError};
use aos_ipc::msg::Status;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;

const DEFAULT_FROM: &str = "service:bridge";
const DEFAULT_BUS: &str = "127.0.0.1:24701";
const DEFAULT_PORT: u16 = 24710;

#[derive(Clone)]
struct AppState {
    bus_addr: String,
    clients: Arc<Mutex<HashMap<String, Arc<BusClient>>>>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    if std::env::args().any(|a| a == "--help" || a == "-h") {
        eprintln!(
            "aos-bridged — loopback HTTP↔bus bridge (E8)\n\
             \n\
             Env:\n\
               AOS_BRIDGE_PORT   listen port (default {DEFAULT_PORT}; always 127.0.0.1)\n\
               AOS_BUS_ADDR      bus host:port (default {DEFAULT_BUS})\n\
             \n\
             Header X-Aos-From → Intent.from (default {DEFAULT_FROM}).\n\
             Opt-in only; not started by aos-session."
        );
        return Ok(());
    }

    let port: u16 = std::env::var("AOS_BRIDGE_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_PORT);
    let bus_addr = std::env::var("AOS_BUS_ADDR").unwrap_or_else(|_| DEFAULT_BUS.into());

    let state = AppState {
        bus_addr: bus_addr.clone(),
        clients: Arc::new(Mutex::new(HashMap::new())),
    };

    let app = Router::new()
        .route("/v1/health", get(health))
        .route("/v1/mem/stats", post(|s, h, b| dispatch(s, h, b, "mem.stats")))
        .route(
            "/v1/mem/working_set",
            post(|s, h, b| dispatch(s, h, b, "mem.working_set")),
        )
        .route(
            "/v1/mem/working_get",
            post(|s, h, b| dispatch(s, h, b, "mem.working_get")),
        )
        .route(
            "/v1/mem/episodic_write",
            post(|s, h, b| dispatch(s, h, b, "mem.episodic_write")),
        )
        .route(
            "/v1/mem/episodic_query",
            post(|s, h, b| dispatch(s, h, b, "mem.episodic_query")),
        )
        .route(
            "/v1/mem/context",
            post(|s, h, b| dispatch(s, h, b, "mem.context")),
        )
        .route(
            "/v1/mem/user/remember",
            post(|s, h, b| dispatch(s, h, b, "mem.user.remember")),
        )
        .route(
            "/v1/mem/user/recall",
            post(|s, h, b| dispatch(s, h, b, "mem.user.recall")),
        )
        .route(
            "/v1/secrets/list",
            post(|s, h, b| dispatch(s, h, b, "secrets.list")),
        )
        .route(
            "/v1/secrets/get",
            post(|s, h, b| dispatch(s, h, b, "secrets.get")),
        )
        .route(
            "/v1/secrets/set",
            post(|s, h, b| dispatch(s, h, b, "secrets.set")),
        )
        .fallback(|| async {
            (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "unknown route" })),
            )
        })
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    tracing::info!("aos-bridged listening on http://{addr}/v1 (bus → {bus_addr})");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health(State(state): State<AppState>) -> impl IntoResponse {
    Json(json!({
        "ok": true,
        "service": "aos-bridged",
        "bus": state.bus_addr,
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

async fn dispatch(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Option<Json<Value>>,
    intent: &'static str,
) -> Response {
    let payload = body.map(|j| j.0).unwrap_or_else(|| json!({}));
    forward_intent(&state, &headers, intent, payload).await
}

async fn forward_intent(
    state: &AppState,
    headers: &HeaderMap,
    intent: &str,
    payload: Value,
) -> Response {
    let from = headers
        .get("X-Aos-From")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_FROM)
        .to_string();

    let client = match client_for(state, &from).await {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": format!("bus connect: {e}") })),
            )
                .into_response();
        }
    };

    match client
        .call_from::<Value, Value>(&from, intent, &payload, Vec::new())
        .await
    {
        Ok(resp) => (StatusCode::OK, Json(resp)).into_response(),
        Err(CallError::Status { status, message }) => {
            let code = match status {
                Status::PermissionDenied => StatusCode::FORBIDDEN,
                Status::NotFound => StatusCode::NOT_FOUND,
                Status::BadRequest => StatusCode::BAD_REQUEST,
                _ => StatusCode::BAD_GATEWAY,
            };
            (
                code,
                Json(json!({ "error": message, "status": format!("{status:?}") })),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn client_for(state: &AppState, from: &str) -> Result<Arc<BusClient>, CallError> {
    let mut map = state.clients.lock().await;
    if let Some(c) = map.get(from) {
        return Ok(c.clone());
    }
    let c = BusClient::connect(&state.bus_addr, from).await?;
    map.insert(from.to_string(), c.clone());
    Ok(c)
}
