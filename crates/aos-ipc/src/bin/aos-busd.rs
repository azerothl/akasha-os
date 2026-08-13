//! `aos-busd` — broker du Semantic IPC Bus (P1.5).
//!
//! Usage : `aos-busd [port]` (défaut 24701).

use aos_ipc::{broker, DEFAULT_BUS_PORT};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let port = std::env::args()
        .nth(1)
        .and_then(|a| a.parse::<u16>().ok())
        .unwrap_or(DEFAULT_BUS_PORT);
    let addr = format!("127.0.0.1:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    eprintln!("[aos-busd] écoute sur {addr}");
    broker::serve(listener).await
}
