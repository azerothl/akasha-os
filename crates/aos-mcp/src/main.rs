//! `aos-mcpd` — opt-in stdio MCP server over the Akasha intent bus.
//!
//! Not spawned by `aos-session`. Point Claude Code / Codex / Cursor / Copilot
//! at this binary while Preview is running (bus on `AOS_BUS_ADDR`).

use aos_mcp::{handle_message, BusState, JsonRpcRequest, DEFAULT_BUS, DEFAULT_FROM, SERVER_NAME};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // MCP forbids non-protocol bytes on stdout — log to stderr only.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    if std::env::args().any(|a| a == "--help" || a == "-h") {
        eprintln!(
            "{SERVER_NAME} — stdio MCP façade (mem + infer) over the Akasha bus\n\
             \n\
             Env:\n\
               AOS_BUS_ADDR   bus host:port (default {DEFAULT_BUS})\n\
               AOS_MCP_FROM   Intent.from (default {DEFAULT_FROM})\n\
             \n\
             Opt-in only; not started by aos-session. Preview must be running.\n\
             See docs/mcp-server.md."
        );
        return Ok(());
    }

    let bus = BusState::from_env();
    tracing::info!(
        "aos-mcpd ready (stdio MCP → bus {} as {})",
        bus.bus_addr(),
        bus.from()
    );

    let stdin = BufReader::new(tokio::io::stdin());
    let mut lines = stdin.lines();
    let mut stdout = tokio::io::stdout();

    while let Some(line) = lines.next_line().await? {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let req: JsonRpcRequest = match serde_json::from_str(trimmed) {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("invalid JSON-RPC line: {e}");
                let resp = aos_mcp::JsonRpcResponse::error(None, -32700, format!("parse error: {e}"));
                write_message(&mut stdout, &resp).await?;
                continue;
            }
        };

        if let Some(resp) = handle_message(&bus, req).await {
            write_message(&mut stdout, &resp).await?;
        }
    }

    tracing::info!("aos-mcpd stdin closed — exit");
    Ok(())
}

async fn write_message<W: AsyncWriteExt + Unpin>(
    out: &mut W,
    resp: &aos_mcp::JsonRpcResponse,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut line = serde_json::to_string(resp)?;
    line.push('\n');
    out.write_all(line.as_bytes()).await?;
    out.flush().await?;
    Ok(())
}
