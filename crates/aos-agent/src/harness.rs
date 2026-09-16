//! Allowlisted external coding harnesses (`harness.run`).
//!
//! Phase 1: spawn `codex` / `claude` / `grok` with a fixed argv template, never a
//! shell, never model-supplied extra flags. Confirmation is the act-gate.
//!
//! Phase 2 (not implemented): a real external agent backend that appears on the
//! roster and maps pause / resume / steer / kill onto the child process.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

const DEFAULT_TIMEOUT_SECS: u64 = 180;
const MAX_TIMEOUT_SECS: u64 = 600;
const MIN_TIMEOUT_SECS: u64 = 15;
const MAX_PROMPT_CHARS: usize = 24_000;
const MAX_OUTPUT_CHARS: usize = 32_000;
const CAP: &str = "harness.run";

/// Allowlisted CLI harness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HarnessKind {
    Codex,
    Claude,
    Grok,
}

impl HarnessKind {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "codex" => Some(Self::Codex),
            "claude" | "claude-code" => Some(Self::Claude),
            "grok" | "grok-bot" => Some(Self::Grok),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
            Self::Grok => "grok",
        }
    }

    pub fn bin_name(self) -> &'static str {
        self.as_str()
    }

    /// Fixed argv; the prompt is the only user-controlled token.
    pub fn argv(self, prompt: &str) -> Vec<String> {
        match self {
            Self::Codex => vec![
                "exec".into(),
                "--skip-git-repo-check".into(),
                prompt.to_string(),
            ],
            Self::Claude => vec![
                "-p".into(),
                prompt.to_string(),
                "--output-format".into(),
                "text".into(),
            ],
            Self::Grok => vec!["-p".into(), prompt.to_string()],
        }
    }
}

pub fn has_harness_cap(caps: &[String]) -> bool {
    caps.iter().any(|c| c == CAP || c == "harness.run:*")
}

/// Validate args and return a structured request (no spawn).
pub fn parse_request(args: &Value) -> Result<(HarnessKind, String, Option<String>, u64), String> {
    if args.get("argv").is_some() || args.get("args").is_some() || args.get("command").is_some() {
        return Err(
            "harness.run refuse argv/command libres — seulement harness + prompt (+ cwd, timeout_sec)"
                .into(),
        );
    }
    let kind = args
        .get("harness")
        .and_then(|v| v.as_str())
        .and_then(HarnessKind::parse)
        .ok_or_else(|| {
            "harness.run : harness requis (codex | claude | grok)".to_string()
        })?;
    let prompt = args
        .get("prompt")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if prompt.is_empty() {
        return Err("harness.run : prompt requis".into());
    }
    if prompt.chars().count() > MAX_PROMPT_CHARS {
        return Err(format!(
            "harness.run : prompt trop long (max {MAX_PROMPT_CHARS} caractères)"
        ));
    }
    let cwd = args
        .get("cwd")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    if let Some(c) = cwd.as_deref() {
        if c.contains('\0') || c.contains('\n') || c.contains('\r') {
            return Err("harness.run : cwd invalide".into());
        }
    }
    let timeout_sec = args
        .get("timeout_sec")
        .and_then(|v| v.as_u64())
        .unwrap_or(DEFAULT_TIMEOUT_SECS)
        .clamp(MIN_TIMEOUT_SECS, MAX_TIMEOUT_SECS);
    Ok((kind, prompt, cwd, timeout_sec))
}

fn find_harness_binary(kind: HarnessKind) -> Result<PathBuf, String> {
    let name = kind.bin_name();
    let mut names = vec![name.to_string()];
    if cfg!(windows) {
        names.push(format!("{name}.exe"));
        names.push(format!("{name}.cmd"));
    }
    let path_os = std::env::var_os("PATH").ok_or_else(|| {
        format!("harness.run : {name} introuvable (PATH vide) — installe le CLI localement")
    })?;
    for dir in std::env::split_paths(&path_os) {
        for candidate_name in &names {
            let candidate = dir.join(candidate_name);
            if !candidate.is_file() {
                continue;
            }
            if !stem_matches(&candidate, name) {
                continue;
            }
            return Ok(candidate);
        }
    }
    Err(format!(
        "harness.run : binaire `{name}` absent du PATH — installe Codex / Claude Code / Grok CLI"
    ))
}

fn stem_matches(path: &Path, harness: &str) -> bool {
    path.file_stem()
        .and_then(|s| s.to_str())
        .is_some_and(|stem| stem.eq_ignore_ascii_case(harness))
}

fn resolve_cwd(raw: Option<&str>) -> Result<PathBuf, String> {
    let base = std::env::var_os("AOS_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .ok_or_else(|| "harness.run : cwd par défaut introuvable".to_string())?;
    let path = match raw {
        None | Some("") => base,
        Some(p) => {
            let p = PathBuf::from(p);
            if p.is_absolute() {
                p
            } else {
                base.join(p)
            }
        }
    };
    let canon = path
        .canonicalize()
        .map_err(|e| format!("harness.run : cwd inaccessible ({e})"))?;
    if !canon.is_dir() {
        return Err("harness.run : cwd n'est pas un dossier".into());
    }
    Ok(canon)
}

fn truncate_chars(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        return s.to_string();
    }
    format!("{}…\n[tronqué {} caractères]", s.chars().take(max).collect::<String>(), count)
}

/// Run an allowlisted harness. Never uses a shell.
pub async fn run(args: &Value, caps: &[String]) -> String {
    if !has_harness_cap(caps) {
        return "harness.run : capacité manquante (coche Harness sur l'agent)".into();
    }
    let (kind, prompt, cwd_raw, timeout_sec) = match parse_request(args) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let bin = match find_harness_binary(kind) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let cwd = match resolve_cwd(cwd_raw.as_deref()) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let argv = kind.argv(&prompt);
    let mut cmd = Command::new(&bin);
    cmd.args(&argv)
        .current_dir(&cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return format!("harness.run spawn err: {e}"),
    };
    let mut stdout_pipe = child.stdout.take();
    let mut stderr_pipe = child.stderr.take();
    let stdout_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        if let Some(ref mut s) = stdout_pipe {
            let _ = s.read_to_end(&mut buf).await;
        }
        String::from_utf8_lossy(&buf).into_owned()
    });
    let stderr_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        if let Some(ref mut s) = stderr_pipe {
            let _ = s.read_to_end(&mut buf).await;
        }
        String::from_utf8_lossy(&buf).into_owned()
    });
    match tokio::time::timeout(Duration::from_secs(timeout_sec), child.wait()).await {
        Ok(status) => {
            let code = status
                .map(|s| {
                    s.code()
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| "signal".into())
                })
                .unwrap_or_else(|e| format!("wait err: {e}"));
            let out = stdout_task.await.unwrap_or_default();
            let err = stderr_task.await.unwrap_or_default();
            format!(
                "harness={} exit={code}\ncwd={}\n--- stdout ---\n{}\n--- stderr ---\n{}",
                kind.as_str(),
                cwd.display(),
                truncate_chars(&out, MAX_OUTPUT_CHARS),
                truncate_chars(&err, MAX_OUTPUT_CHARS / 4)
            )
        }
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            format!(
                "harness.run : timeout après {timeout_sec}s ({})",
                kind.as_str()
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_allowlist_and_aliases() {
        assert_eq!(HarnessKind::parse("codex"), Some(HarnessKind::Codex));
        assert_eq!(HarnessKind::parse("Claude-Code"), Some(HarnessKind::Claude));
        assert_eq!(HarnessKind::parse("grok-bot"), Some(HarnessKind::Grok));
        assert_eq!(HarnessKind::parse("bash"), None);
        assert_eq!(HarnessKind::parse("../codex"), None);
        assert_eq!(HarnessKind::parse("codex; rm -rf /"), None);
    }

    #[test]
    fn rejects_freeform_argv() {
        let err = parse_request(&serde_json::json!({
            "harness": "codex",
            "prompt": "hi",
            "argv": ["exec", "-c", "id"]
        }))
        .unwrap_err();
        assert!(err.contains("argv"));
    }

    #[test]
    fn requires_prompt_and_known_harness() {
        assert!(parse_request(&serde_json::json!({"harness":"codex"})).is_err());
        assert!(parse_request(&serde_json::json!({"prompt":"x"})).is_err());
        let ok = parse_request(&serde_json::json!({
            "harness": "claude",
            "prompt": "review this file"
        }))
        .unwrap();
        assert_eq!(ok.0, HarnessKind::Claude);
        assert_eq!(ok.3, DEFAULT_TIMEOUT_SECS);
    }

    #[test]
    fn argv_templates_are_fixed() {
        let p = "fix the tests";
        assert_eq!(
            HarnessKind::Codex.argv(p),
            vec!["exec", "--skip-git-repo-check", p]
        );
        assert_eq!(
            HarnessKind::Claude.argv(p),
            vec!["-p", p, "--output-format", "text"]
        );
        assert_eq!(HarnessKind::Grok.argv(p), vec!["-p", p]);
    }

    #[test]
    fn stem_must_match_allowlist() {
        assert!(stem_matches(Path::new("/usr/bin/codex"), "codex"));
        assert!(stem_matches(Path::new(r"C:\Tools\claude.exe"), "claude"));
        assert!(!stem_matches(Path::new("/usr/bin/codex-evil"), "codex"));
        assert!(!stem_matches(Path::new("/bin/sh"), "codex"));
    }

    #[test]
    fn cap_check() {
        assert!(!has_harness_cap(&[]));
        assert!(has_harness_cap(&["harness.run".into()]));
    }
}
