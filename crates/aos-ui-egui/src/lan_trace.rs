//! Append-only LAN UI diagnostic log.
//!
//! Always writes under `%LOCALAPPDATA%/AgentOS-Preview/var/log/lan-ui-trace.log`
//! (plus `AOS_HOME` when set) so a wrong cwd cannot hide the file.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn timestamp_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

fn trace_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        paths.push(
            PathBuf::from(local)
                .join("AgentOS-Preview")
                .join("var")
                .join("log")
                .join("lan-ui-trace.log"),
        );
    }
    if let Ok(home) = std::env::var("AOS_HOME") {
        let p = PathBuf::from(home)
            .join("var")
            .join("log")
            .join("lan-ui-trace.log");
        if !paths.iter().any(|existing| existing == &p) {
            paths.push(p);
        }
    }
    paths
}

pub fn log(event: &str, detail: &str) {
    let line = format!("{} | {event} | {detail}\n", timestamp_ms());
    eprintln!("[lan-ui-trace] {event} | {detail}");
    for path in trace_paths() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) {
            let _ = file.write_all(line.as_bytes());
            let _ = file.flush();
        }
    }
}
