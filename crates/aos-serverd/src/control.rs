//! Local control plane for `aos-serverd` (P21.3–P21.4).
//!
//! Transport: Unix domain socket under `$AOS_HOME/var/run/aos-serverd.sock`
//! (Unix), or loopback TCP whose address is written to
//! `$AOS_HOME/var/run/aos-serverd.pipe` (Windows). Never binds `0.0.0.0`.

use crate::intake::{self, EnqueueParams, IntakeMode, JobRecord};
use crate::{control_socket_path, ControlCommand};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// One-line JSON request on the control channel.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ControlRequest {
    pub cmd: String,
    #[serde(default)]
    pub actor: String,
    /// Goal text for create/schedule intake (P21.4).
    #[serde(default)]
    pub goal: Option<String>,
    /// Existing agent id for `start` mode.
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub model_id: Option<String>,
    /// Caps presented with the bus call (serverd does not mint).
    #[serde(default)]
    pub caps: Vec<String>,
    /// `create` | `start` | `schedule` (default create).
    #[serde(default)]
    pub mode: Option<String>,
    /// Job id for `job-status`.
    #[serde(default)]
    pub job_id: Option<String>,
    /// Schedule interval seconds (schedule mode).
    #[serde(default)]
    pub interval_secs: Option<u64>,
}

impl ControlRequest {
    pub fn parse_line(line: &str) -> Result<Self, String> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Err("empty control request".into());
        }
        if trimmed.starts_with('{') {
            return serde_json::from_str(trimmed).map_err(|e| e.to_string());
        }
        // Plain command word (CLI ergonomics).
        let cmd = trimmed.to_ascii_lowercase();
        match cmd.as_str() {
            "status" | "restart" | "stop" | "job-list" | "jobs" => Ok(Self {
                cmd: if cmd == "jobs" {
                    "job-list".into()
                } else {
                    cmd
                },
                actor: "cli".into(),
                goal: None,
                agent_id: None,
                model_id: None,
                caps: Vec::new(),
                mode: None,
                job_id: None,
                interval_secs: None,
            }),
            other => Err(format!("unknown control command `{other}`")),
        }
    }

    pub fn command(&self) -> Result<ControlCommand, String> {
        match self.cmd.as_str() {
            "status" => Ok(ControlCommand::Status),
            "restart" => Ok(ControlCommand::Restart),
            "stop" => Ok(ControlCommand::Stop),
            "enqueue-agent" | "server.job.enqueue" | "enqueue" => Ok(ControlCommand::EnqueueAgent),
            "job-list" | "server.job.list" | "jobs" => Ok(ControlCommand::JobList),
            "job-status" | "server.job.status" => Ok(ControlCommand::JobStatus),
            other => Err(format!("unsupported control command `{other}`")),
        }
    }

    pub fn into_enqueue_params(&self) -> Result<EnqueueParams, String> {
        let mode = IntakeMode::parse(self.mode.as_deref().unwrap_or("create"))?;
        Ok(EnqueueParams {
            actor: self.actor.clone(),
            goal: self.goal.clone(),
            agent_id: self.agent_id.clone(),
            model_id: self.model_id.clone(),
            caps: self.caps.clone(),
            mode,
            interval_secs: self.interval_secs,
        })
    }
}

/// Snapshot returned by `status` (also useful offline for unit tests).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TreeStatus {
    pub running: bool,
    pub lot: String,
    pub daemons: Vec<DaemonStatus>,
    pub control_plane: bool,
    pub aos_home: String,
    /// Intake jobs currently recorded (P21.4).
    #[serde(default)]
    pub job_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DaemonStatus {
    pub name: String,
    pub alive: bool,
    pub pid: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ControlResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<TreeStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job: Option<JobRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jobs: Option<Vec<JobRecord>>,
}

impl ControlResponse {
    pub fn to_line(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| {
            r#"{"ok":false,"message":"serialize failed"}"#.into()
        })
    }
}

/// Shared control state owned by the serve loop.
pub struct ControlState {
    pub home: std::path::PathBuf,
    pub tree: Arc<Mutex<crate::ProcessTree>>,
    pub opts: crate::SpawnOptions,
    pub stop: Arc<AtomicBool>,
    /// Backoff seconds before the next ordered tree restart (busd/capkd path).
    pub tree_restart_backoff_secs: Mutex<u64>,
}

impl ControlState {
    pub fn snapshot(&self) -> TreeStatus {
        let mut tree = self.tree.lock().unwrap();
        let daemons: Vec<DaemonStatus> = tree
            .daemons_mut()
            .iter_mut()
            .map(|d| {
                let alive = matches!(d.child.try_wait(), Ok(None));
                DaemonStatus {
                    name: d.name.to_string(),
                    alive,
                    pid: if alive { Some(d.child.id()) } else { None },
                }
            })
            .collect();
        TreeStatus {
            running: !daemons.is_empty() && daemons.iter().any(|d| d.alive),
            lot: "P21.6".into(),
            daemons,
            control_plane: true,
            aos_home: self.home.display().to_string(),
            job_count: intake::list_jobs(&self.home).len(),
        }
    }

    pub fn ordered_restart(&self) -> Result<(), String> {
        let wait = *self.tree_restart_backoff_secs.lock().unwrap();
        if wait > 0 {
            eprintln!("[aos-serverd] tree restart backoff {wait}s");
            thread::sleep(Duration::from_secs(wait));
        }
        {
            let mut t = self.tree.lock().unwrap();
            eprintln!("[aos-serverd] ordered restart (audited)");
            crate::log_daemon_restart(&self.home, "aos-serverd-tree", true);
            t.stop();
            t.start(&self.opts)?;
        }
        if let Err(e) = crate::healthcheck() {
            let mut backoff = self.tree_restart_backoff_secs.lock().unwrap();
            let next = (*backoff).saturating_mul(2).clamp(2, 60);
            *backoff = next;
            return Err(format!("healthcheck after restart: {e}"));
        }
        *self.tree_restart_backoff_secs.lock().unwrap() = 2;
        Ok(())
    }

    pub fn handle(&self, req: &ControlRequest) -> ControlResponse {
        let actor = if req.actor.is_empty() {
            "local"
        } else {
            req.actor.as_str()
        };
        match req.command() {
            Ok(ControlCommand::Status) => ControlResponse {
                ok: true,
                message: Some(format!("actor={actor}")),
                status: Some(self.snapshot()),
                job: None,
                jobs: None,
            },
            Ok(ControlCommand::Restart) => match self.ordered_restart() {
                Ok(()) => {
                    crate::log_control_audit(&self.home, actor, "restart", true);
                    ControlResponse {
                        ok: true,
                        message: Some("restarted".into()),
                        status: Some(self.snapshot()),
                        job: None,
                        jobs: None,
                    }
                }
                Err(e) => {
                    crate::log_control_audit(&self.home, actor, "restart", false);
                    ControlResponse {
                        ok: false,
                        message: Some(e),
                        status: Some(self.snapshot()),
                        job: None,
                        jobs: None,
                    }
                }
            },
            Ok(ControlCommand::Stop) => {
                crate::log_control_audit(&self.home, actor, "stop", true);
                self.tree.lock().unwrap().stop();
                self.stop.store(true, Ordering::SeqCst);
                ControlResponse {
                    ok: true,
                    message: Some("stopping".into()),
                    status: None,
                    job: None,
                    jobs: None,
                }
            }
            Ok(ControlCommand::EnqueueAgent) => match req.into_enqueue_params() {
                Ok(params) => match intake::enqueue(&self.home, params) {
                    Ok(job) => {
                        crate::log_control_audit(&self.home, actor, "enqueue-agent", true);
                        ControlResponse {
                            ok: true,
                            message: Some("enqueued".into()),
                            status: None,
                            job: Some(job),
                            jobs: None,
                        }
                    }
                    Err(e) => {
                        crate::log_control_audit(&self.home, actor, "enqueue-agent", false);
                        ControlResponse {
                            ok: false,
                            message: Some(e),
                            status: None,
                            job: None,
                            jobs: None,
                        }
                    }
                },
                Err(e) => ControlResponse {
                    ok: false,
                    message: Some(e),
                    status: None,
                    job: None,
                    jobs: None,
                },
            },
            Ok(ControlCommand::JobList) => ControlResponse {
                ok: true,
                message: None,
                status: None,
                job: None,
                jobs: Some(intake::list_jobs(&self.home)),
            },
            Ok(ControlCommand::JobStatus) => {
                let id = req.job_id.as_deref().unwrap_or("").trim();
                if id.is_empty() {
                    ControlResponse {
                        ok: false,
                        message: Some("job_id required".into()),
                        status: None,
                        job: None,
                        jobs: None,
                    }
                } else if let Some(job) = intake::get_job(&self.home, id) {
                    ControlResponse {
                        ok: true,
                        message: None,
                        status: None,
                        job: Some(job),
                        jobs: None,
                    }
                } else {
                    ControlResponse {
                        ok: false,
                        message: Some(format!("job not found: {id}")),
                        status: None,
                        job: None,
                        jobs: None,
                    }
                }
            }
            Err(e) => ControlResponse {
                ok: false,
                message: Some(e),
                status: None,
                job: None,
                jobs: None,
            },
        }
    }
}

/// Send one control request to a running serverd; returns the response line JSON.
pub fn send_control(home: &Path, req: &ControlRequest) -> Result<ControlResponse, String> {
    let line = serde_json::to_string(req).map_err(|e| e.to_string())?;
    let raw = send_raw(home, &line)?;
    serde_json::from_str(&raw).map_err(|e| format!("bad control response: {e} ({raw})"))
}

fn send_raw(home: &Path, line: &str) -> Result<String, String> {
    #[cfg(unix)]
    {
        use std::os::unix::net::UnixStream;
        let path = control_socket_path(home);
        let mut stream = UnixStream::connect(&path)
            .map_err(|e| format!("connect {}: {e}", path.display()))?;
        stream
            .write_all(format!("{line}\n").as_bytes())
            .map_err(|e| e.to_string())?;
        stream.flush().map_err(|e| e.to_string())?;
        let mut reader = BufReader::new(stream);
        let mut resp = String::new();
        reader.read_line(&mut resp).map_err(|e| e.to_string())?;
        Ok(resp.trim().to_string())
    }
    #[cfg(windows)]
    {
        use std::net::TcpStream;
        let marker = control_socket_path(home);
        let addr = std::fs::read_to_string(&marker)
            .map_err(|e| format!("read {}: {e}", marker.display()))?
            .trim()
            .to_string();
        if addr.is_empty() {
            return Err("empty control address marker".into());
        }
        let mut stream =
            TcpStream::connect(&addr).map_err(|e| format!("connect {addr}: {e}"))?;
        stream
            .write_all(format!("{line}\n").as_bytes())
            .map_err(|e| e.to_string())?;
        stream.flush().map_err(|e| e.to_string())?;
        let mut reader = BufReader::new(stream);
        let mut resp = String::new();
        reader.read_line(&mut resp).map_err(|e| e.to_string())?;
        Ok(resp.trim().to_string())
    }
}

/// Serve control connections until `stop` is set.
pub fn serve_control(state: Arc<ControlState>) {
    #[cfg(unix)]
    {
        use std::os::unix::net::UnixListener;
        let path = control_socket_path(&state.home);
        let _ = std::fs::remove_file(&path);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let listener = match UnixListener::bind(&path) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("[aos-serverd] control bind {}: {e}", path.display());
                return;
            }
        };
        let _ = listener.set_nonblocking(true);
        eprintln!("[aos-serverd] control listening on {}", path.display());
        while !state.stop.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((stream, _)) => {
                    handle_stream(stream, &state);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(100));
                }
                Err(e) => {
                    eprintln!("[aos-serverd] control accept: {e}");
                    thread::sleep(Duration::from_millis(200));
                }
            }
        }
        let _ = std::fs::remove_file(&path);
    }
    #[cfg(windows)]
    {
        use std::net::TcpListener;
        let listener = match TcpListener::bind("127.0.0.1:0") {
            Ok(l) => l,
            Err(e) => {
                eprintln!("[aos-serverd] control bind loopback: {e}");
                return;
            }
        };
        let addr = match listener.local_addr() {
            Ok(a) => a.to_string(),
            Err(e) => {
                eprintln!("[aos-serverd] control local_addr: {e}");
                return;
            }
        };
        let marker = control_socket_path(&state.home);
        if let Some(parent) = marker.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&marker, &addr) {
            eprintln!("[aos-serverd] write {}: {e}", marker.display());
            return;
        }
        let _ = listener.set_nonblocking(true);
        eprintln!(
            "[aos-serverd] control listening on {addr} (marker {})",
            marker.display()
        );
        while !state.stop.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((stream, _)) => handle_stream(stream, &state),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(100));
                }
                Err(e) => {
                    eprintln!("[aos-serverd] control accept: {e}");
                    thread::sleep(Duration::from_millis(200));
                }
            }
        }
        let _ = std::fs::remove_file(&marker);
    }
}

fn handle_stream<S: std::io::Read + std::io::Write>(mut stream: S, state: &ControlState) {
    let mut reader = BufReader::new(&mut stream);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return;
    }
    let resp = match ControlRequest::parse_line(&line) {
        Ok(req) => state.handle(&req),
        Err(e) => ControlResponse {
            ok: false,
            message: Some(e),
            status: None,
            job: None,
            jobs: None,
        },
    };
    let out = format!("{}\n", resp.to_line());
    let _ = stream.write_all(out.as_bytes());
    let _ = stream.flush();
}

/// True when a live control endpoint appears present under `AOS_HOME`.
pub fn control_endpoint_present(home: &Path) -> bool {
    let path = control_socket_path(home);
    #[cfg(unix)]
    {
        path.exists()
    }
    #[cfg(windows)]
    {
        path.exists()
            && std::fs::read_to_string(&path)
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_and_json_status() {
        let a = ControlRequest::parse_line("status").unwrap();
        assert_eq!(a.cmd, "status");
        let b = ControlRequest::parse_line(r#"{"cmd":"restart","actor":"ops"}"#).unwrap();
        assert_eq!(b.cmd, "restart");
        assert_eq!(b.actor, "ops");
        assert!(ControlRequest::parse_line("enqueue").is_err());
        let e = ControlRequest::parse_line(
            r#"{"cmd":"enqueue-agent","actor":"ops","goal":"hi"}"#,
        )
        .unwrap();
        assert_eq!(e.command().unwrap(), ControlCommand::EnqueueAgent);
        assert_eq!(e.goal.as_deref(), Some("hi"));
    }

    #[test]
    fn response_line_is_json() {
        let r = ControlResponse {
            ok: true,
            message: Some("hi".into()),
            status: None,
            job: None,
            jobs: None,
        };
        let line = r.to_line();
        assert!(line.contains("\"ok\":true"));
    }
}
