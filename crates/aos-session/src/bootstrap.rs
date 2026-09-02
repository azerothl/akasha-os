//! Premier run : disque, NVIDIA, modèles GGUF, skills.

use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;

fn emit_download_progress(pct: u64, done: u64, total: u64) {
    eprintln!("[aos-session]   {pct}% ({done}/{total})");
    let _ = std::io::stderr().flush();
}

const MIN_FREE_BYTES: u64 = 8 * 1024 * 1024 * 1024; // ~8 Go (packs mid)

#[derive(Debug, Deserialize)]
struct ModelsManifest {
    models: Vec<ModelEntry>,
}

#[derive(Debug, Deserialize)]
struct ModelEntry {
    filename: String,
    bytes: Option<u64>,
    #[serde(default)]
    sha256: String,
    url: String,
}

pub fn nvidia_ok() -> bool {
    Command::new("nvidia-smi")
        .arg("-L")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(target_os = "macos")]
pub fn apple_silicon_host() -> bool {
    std::env::consts::ARCH == "aarch64"
}

/// GPU-backed inference is available on this host (NVIDIA on Win/Linux, Metal on Apple Silicon).
pub fn gpu_accel_ok() -> bool {
    #[cfg(target_os = "macos")]
    {
        return apple_silicon_host();
    }
    #[cfg(not(target_os = "macos"))]
    {
        nvidia_ok()
    }
}

pub fn show_fatal_dialog(title: &str, body: &str) {
    eprintln!("[aos-session] {title}\n{body}");
    #[cfg(windows)]
    {
        let safe_title = title.replace('\'', "''");
        let safe_body = body.replace('\'', "''").replace('\n', "`n");
        let ps = format!(
            "Add-Type -AssemblyName PresentationFramework; [System.Windows.MessageBox]::Show('{safe_body}','{safe_title}') | Out-Null"
        );
        let _ = Command::new("powershell")
            .args(["-NoProfile", "-Command", &ps])
            .status();
    }
    #[cfg(target_os = "macos")]
    {
        // Pass title/body as osascript argv — avoids broken dialogs when body has newlines/quotes.
        let _ = Command::new("osascript")
            .args([
                "-e",
                "on run argv",
                "-e",
                "display dialog (item 2 of argv) with title (item 1 of argv) buttons {\"OK\"} default button 1 with icon caution",
                "-e",
                "end run",
            ])
            .arg(title)
            .arg(body)
            .status();
    }
}

/// Boot-failure dialog: one-line body + localized Retry button. Returns `true` to retry.
pub fn show_bootstrap_retry_dialog(title: &str, body: &str, retry_label: &str) -> bool {
    eprintln!("[aos-session] {title}\n{body}");
    #[cfg(test)]
    {
        eprintln!("[aos-session] {retry_label} (test — auto-retry)");
        true
    }
    #[cfg(all(not(test), windows))]
    {
        let safe_title = ps_escape_single_quoted(title);
        let safe_body = ps_escape_single_quoted(body);
        let safe_retry = ps_escape_single_quoted(retry_label);
        let ps = format!(
            r#"Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
$form = New-Object System.Windows.Forms.Form
$form.Text = '{safe_title}'
$form.FormBorderStyle = 'FixedDialog'
$form.MaximizeBox = $false
$form.MinimizeBox = $false
$form.StartPosition = 'CenterScreen'
$form.ClientSize = New-Object System.Drawing.Size(420, 140)
$label = New-Object System.Windows.Forms.Label
$label.Text = '{safe_body}'
$label.AutoSize = $false
$label.Size = New-Object System.Drawing.Size(380, 50)
$label.Location = New-Object System.Drawing.Point(20, 20)
$form.Controls.Add($label)
$btn = New-Object System.Windows.Forms.Button
$btn.Text = '{safe_retry}'
$btn.Size = New-Object System.Drawing.Size(120, 30)
$btn.Location = New-Object System.Drawing.Point(150, 80)
$btn.DialogResult = [System.Windows.Forms.DialogResult]::OK
$form.AcceptButton = $btn
$form.Controls.Add($btn)
$form.Add_Shown({{ $form.Activate(); $btn.Focus() }})
if ($form.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {{ exit 0 }} else {{ exit 1 }}"#
        );
        Command::new("powershell")
            .args(["-NoProfile", "-Command", &ps])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(all(not(test), target_os = "macos"))]
    {
        let out = Command::new("osascript")
            .args([
                "-e",
                "on run argv",
                "-e",
                "set dlg to display dialog (item 2 of argv) with title (item 1 of argv) buttons {item 3 of argv} default button 1 with icon caution",
                "-e",
                "return button returned of dlg",
                "-e",
                "end run",
            ])
            .arg(title)
            .arg(body)
            .arg(retry_label)
            .output();
        return out
            .map(|o| String::from_utf8_lossy(&o.stdout).trim() == retry_label)
            .unwrap_or(false);
    }
    #[cfg(all(not(test), not(any(windows, target_os = "macos"))))]
    {
        eprintln!("[aos-session] {retry_label} (headless — auto-retry)");
        let _ = (title, body);
        true
    }
}

#[cfg(all(windows, not(test)))]
fn ps_escape_single_quoted(s: &str) -> String {
    s.replace('\'', "''")
}

pub fn check_disk_space(home: &Path) -> Result<(), String> {
    let target = if home.exists() {
        home.to_path_buf()
    } else {
        PathBuf::from(".")
    };
    #[cfg(windows)]
    {
        // PowerShell free space on the drive of AOS_HOME
        let drive = target
            .components()
            .next()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .unwrap_or_else(|| "C:\\".into());
        let out = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!(
                    "(Get-PSDrive -Name '{d}').Free",
                    d = drive.trim_end_matches('\\').trim_end_matches(':')
                ),
            ])
            .output()
            .map_err(|e| e.to_string())?;
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if let Ok(free) = s.parse::<u64>() {
            if free < MIN_FREE_BYTES {
                return Err(format!(
                    "espace disque insuffisant (~{:.1} Go libres, ~8 Go requis)",
                    free as f64 / (1 << 30) as f64
                ));
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(stat) = fs::metadata(&target) {
            let _ = stat;
        }
        // statvfs via `df -B1`
        let out = Command::new("df")
            .args(["-B1", target.to_str().unwrap_or(".")])
            .output()
            .map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&out.stdout);
        if let Some(line) = text.lines().nth(1) {
            let cols: Vec<_> = line.split_whitespace().collect();
            if cols.len() >= 4 {
                if let Ok(avail) = cols[3].parse::<u64>() {
                    if avail < MIN_FREE_BYTES {
                        return Err(format!(
                            "espace disque insuffisant (~{:.1} Go libres, ~8 Go requis)",
                            avail as f64 / (1 << 30) as f64
                        ));
                    }
                }
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        let out = Command::new("df")
            .args(["-kP", target.to_str().unwrap_or(".")])
            .output()
            .map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&out.stdout);
        if let Some(line) = text.lines().nth(1) {
            let cols: Vec<_> = line.split_whitespace().collect();
            if cols.len() >= 4 {
                if let Ok(avail_kib) = cols[3].parse::<u64>() {
                    let avail = avail_kib * 1024;
                    if avail < MIN_FREE_BYTES {
                        return Err(format!(
                            "espace disque insuffisant (~{:.1} Go libres, ~8 Go requis)",
                            avail as f64 / (1 << 30) as f64
                        ));
                    }
                }
            }
        }
    }
    let _ = target;
    Ok(())
}

pub fn ensure_skills(home: &Path) {
    let dest = home.join("skills");
    let _ = fs::create_dir_all(&dest);
    for cand in [
        home.join("share/skills"),
        PathBuf::from("share/skills"),
        PathBuf::from("skills"),
    ] {
        if !cand.is_dir() {
            continue;
        }
        // Sync : copie les skills manquantes / met à jour depuis le package.
        let mut copied = 0u32;
        if let Ok(rd) = fs::read_dir(&cand) {
            for ent in rd.filter_map(|e| e.ok()) {
                let src = ent.path();
                if !src.is_dir() {
                    continue;
                }
                let name = match src.file_name() {
                    Some(n) => n.to_os_string(),
                    None => continue,
                };
                let target = dest.join(&name);
                // Toujours re-synchroniser depuis share/ (package à jour).
                let _ = fs::remove_dir_all(&target);
                if copy_dir_recursive(&src, &target).is_ok() {
                    copied += 1;
                }
            }
        }
        if copied > 0 {
            eprintln!(
                "[aos-session] {copied} skill(s) synchronisée(s) depuis {}",
                cand.display()
            );
            return;
        }
    }
}

/// Première install : stub MCP si absent.
pub fn ensure_mcp_stub(home: &Path) {
    let dir = home.join("var/mcp");
    let _ = fs::create_dir_all(&dir);
    let cfg = dir.join("servers.yaml");
    if cfg.exists() {
        return;
    }
    for cand in [
        home.join("share/mcp/servers.yaml.example"),
        PathBuf::from("share/mcp/servers.yaml.example"),
        PathBuf::from("var/mcp/servers.yaml.example"),
    ] {
        if cand.is_file() {
            let _ = fs::copy(&cand, &cfg);
            eprintln!("[aos-session] MCP stub depuis {}", cand.display());
            return;
        }
    }
    let _ = fs::write(
        &cfg,
        "# MCP servers (stdio). See share/mcp/servers.yaml.example\n\nservers: {}\n",
    );
}

pub fn ensure_models(home: &Path) -> Result<(), String> {
    let manifest_path = home.join("share/models/manifest.json");
    if !manifest_path.exists() {
        eprintln!("[aos-session] pas de share/models/manifest.json — skip download");
        return Ok(());
    }
    let raw = fs::read_to_string(&manifest_path).map_err(|e| e.to_string())?;
    let man: ModelsManifest = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    let dir = home.join("share/models");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    for m in &man.models {
        let path = dir.join(&m.filename);
        if model_ok(&path, m)? {
            eprintln!("[aos-session] modèle OK {}", m.filename);
            continue;
        }
        eprintln!(
            "[aos-session] téléchargement {} (~{:.1} Go)…",
            m.filename,
            m.bytes.unwrap_or(0) as f64 / (1 << 30) as f64
        );
        download_model_file(&m.url, &path, m.bytes, &m.sha256)?;
        eprintln!("[aos-session] téléchargé {}", m.filename);
    }
    Ok(())
}

/// Télécharge un GGUF (resume + sha256 optionnel). Utilisé par offerings.
pub fn download_model_file(
    url: &str,
    dest: &Path,
    expected: Option<u64>,
    sha256: &str,
) -> Result<(), String> {
    if dest.exists() {
        if let Some(expect) = expected {
            if let Ok(meta) = fs::metadata(dest) {
                let lo = expect.saturating_mul(99) / 100;
                if meta.len() >= lo {
                    if sha256.is_empty() {
                        return Ok(());
                    }
                    let got = file_sha256(dest)?;
                    if got.eq_ignore_ascii_case(sha256) {
                        return Ok(());
                    }
                }
            }
        } else if sha256.is_empty() {
            return Ok(());
        }
    }
    eprintln!(
        "[aos-session] téléchargement {} (~{:.1} Go)…",
        dest.file_name().and_then(|s| s.to_str()).unwrap_or("model"),
        expected.unwrap_or(0) as f64 / (1 << 30) as f64
    );
    download_file(url, dest, expected)?;
    if !sha256.is_empty() {
        let got = file_sha256(dest)?;
        if !got.eq_ignore_ascii_case(sha256) {
            let _ = fs::remove_file(dest);
            return Err(format!(
                "sha256 invalide pour {} (attendu {sha256}, obtenu {got})",
                dest.display()
            ));
        }
    }
    Ok(())
}

fn model_ok(path: &Path, m: &ModelEntry) -> Result<bool, String> {
    if !path.exists() {
        return Ok(false);
    }
    let meta = fs::metadata(path).map_err(|e| e.to_string())?;
    if let Some(expect) = m.bytes {
        // tolérance 1 %
        let lo = expect.saturating_mul(99) / 100;
        if meta.len() < lo {
            return Ok(false);
        }
    }
    if !m.sha256.is_empty() {
        let got = file_sha256(path)?;
        return Ok(got.eq_ignore_ascii_case(&m.sha256));
    }
    Ok(meta.len() > 1024)
}

fn file_sha256(path: &Path) -> Result<String, String> {
    let mut f = File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 1024 * 256];
    loop {
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn download_file(url: &str, dest: &Path, expected: Option<u64>) -> Result<(), String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .user_agent("akasha-os-preview")
        .build()
        .map_err(|e| e.to_string())?;
    let tmp = dest.with_extension("partial");
    let mut resume_from = 0u64;
    if tmp.exists() {
        resume_from = fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0);
    }
    let mut req = client.get(url);
    if resume_from > 0 {
        req = req.header("Range", format!("bytes={resume_from}-"));
    }
    let mut resp = req.send().map_err(|e| e.to_string())?;
    if !(resp.status().is_success() || resp.status().as_u16() == 206) {
        return Err(format!("HTTP {} pour {url}", resp.status()));
    }
    let mut file = if resume_from > 0 && resp.status().as_u16() == 206 {
        fs::OpenOptions::new()
            .append(true)
            .open(&tmp)
            .map_err(|e| e.to_string())?
    } else {
        resume_from = 0;
        File::create(&tmp).map_err(|e| e.to_string())?
    };
    let total = expected.or_else(|| resp.content_length().map(|l| l + resume_from));
    let mut done = resume_from;
    let mut buf = [0u8; 1024 * 64];
    let mut last_pct = 0u64;
    use std::io::Read as _;
    loop {
        let n = resp.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        done += n as u64;
        if let Some(t) = total {
            if let Some(pct) = done.checked_mul(100).and_then(|n| n.checked_div(t)) {
                if pct >= last_pct + 5 || pct == 100 {
                    emit_download_progress(pct.min(100), done, t);
                    last_pct = pct;
                }
            }
        }
    }
    drop(file);
    fs::rename(&tmp, dest).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn copy_dir_recursive(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst).map_err(|e| annotate_io(dst, e))?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &to)?;
        } else {
            replace_file(&entry.path(), &to)?;
        }
    }
    Ok(())
}

fn annotate_io(path: &Path, err: io::Error) -> io::Error {
    io::Error::new(err.kind(), format!("{}: {err}", path.display()))
}

pub(crate) fn is_lock_error(err: &io::Error) -> bool {
    matches!(
        err.raw_os_error(),
        Some(5 | 32 | 33) // ERROR_ACCESS_DENIED / SHARING_VIOLATION / LOCK_VIOLATION
        | Some(16 | 26) // EBUSY / ETXTBSY
    )
}

fn default_old_sidecar(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".old");
    path.with_file_name(name)
}

fn old_sidecar(path: &Path) -> PathBuf {
    let candidate = default_old_sidecar(path);
    if !candidate.exists() {
        return candidate;
    }
    let mut name = candidate.file_name().unwrap_or_default().to_os_string();
    name.push(".");
    name.push(std::process::id().to_string());
    path.with_file_name(name)
}

/// Copy `src` onto `dst`. If `dst` is a running image (Windows error 32),
/// rename it to `*.old` first — Windows allows renaming a mapped executable.
pub fn replace_file(src: &Path, dst: &Path) -> io::Result<()> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent).map_err(|e| annotate_io(parent, e))?;
    }
    try_copy_with_retry(src, dst)
        .map(|_| ())
        .map_err(|e| annotate_io(dst, e))
}

fn try_copy_with_retry(src: &Path, dst: &Path) -> io::Result<u64> {
    let mut last = None;
    for attempt in 0..8u32 {
        match fs::copy(src, dst) {
            Ok(n) => {
                let _ = fs::remove_file(default_old_sidecar(dst));
                return Ok(n);
            }
            Err(e) if is_lock_error(&e) => {
                let old = old_sidecar(dst);
                let _ = fs::remove_file(&old);
                if dst.exists() {
                    if let Err(ren) = fs::rename(dst, &old) {
                        last = Some(ren);
                    } else {
                        match fs::copy(src, dst) {
                            Ok(n) => {
                                let _ = fs::remove_file(&old);
                                return Ok(n);
                            }
                            Err(e2) => {
                                let _ = fs::rename(&old, dst);
                                last = Some(e2);
                            }
                        }
                    }
                } else {
                    last = Some(e);
                }
                thread::sleep(Duration::from_millis(50 * (1 << attempt.min(4))));
            }
            Err(e) => return Err(e),
        }
    }
    Err(last.unwrap_or_else(|| io::Error::other("copy retry exhausted")))
}

/// Drop leftover `*.old` sidecars from a previous in-place binary replace.
pub fn sweep_old_sidecars(dir: &Path) {
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    for ent in rd.filter_map(|e| e.ok()) {
        let name = ent.file_name();
        if name.to_string_lossy().contains(".old") {
            let _ = fs::remove_file(ent.path());
        }
    }
}

fn module_manifest_hash(dir: &Path) -> Option<String> {
    let raw = fs::read_to_string(dir.join("manifest.yaml")).ok()?;
    for line in raw.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("hash:") {
            let h = rest.trim().trim_matches('"').trim_matches('\'').to_string();
            if !h.is_empty() {
                return Some(h);
            }
        }
    }
    None
}

fn wasm_sha256(dir: &Path) -> Option<String> {
    let data = fs::read(dir.join("module.wasm")).ok()?;
    use sha2::{Digest, Sha256};
    Some(format!("{:x}", Sha256::digest(&data)))
}

fn wasm_fingerprint(dir: &Path) -> Option<(u64, u64)> {
    let meta = fs::metadata(dir.join("module.wasm")).ok()?;
    let len = meta.len();
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Some((len, mtime))
}

const NOTES_REGISTRY_ENTRY: &str = r#"
  - name: notes
    granted_caps:
      - fs.read:/documents/notes/**
      - fs.write:/documents/notes/**
      - mem.write:module:notes
      - mem.query:module:notes
    quarantined: false
"#;

/// Ensure the bundled notes module is registered when its WASM is on disk.
/// Upgrades from early Preview builds may have synced `var/modules/notes` without
/// adding a registry row (issue #111).
pub fn ensure_notes_registry_entry(registry_path: &Path) {
    if !registry_path
        .parent()
        .is_some_and(|p| p.join("notes/module.wasm").exists())
    {
        return;
    }
    if let Ok(raw) = fs::read_to_string(registry_path) {
        if raw.contains("name: notes") {
            return;
        }
        let mut updated = raw;
        if !updated.ends_with('\n') {
            updated.push('\n');
        }
        updated.push_str(NOTES_REGISTRY_ENTRY);
        let _ = fs::write(registry_path, updated);
        return;
    }
    let _ = fs::write(
        registry_path,
        format!("installed:{NOTES_REGISTRY_ENTRY}"),
    );
}

/// Copie `share/.../*.aospkg` → `var/modules/<name>` si absent ou obsolète.
/// Retourne `true` si une copie a été effectuée.
pub fn sync_packaged_module(share_pkg: &Path, installed_dir: &Path) -> bool {
    if !share_pkg.is_dir() || !share_pkg.join("module.wasm").exists() {
        return false;
    }
    let need = if !installed_dir.join("module.wasm").exists() {
        true
    } else {
        let share_manifest = module_manifest_hash(share_pkg);
        let inst_manifest = module_manifest_hash(installed_dir);
        let share_wasm = wasm_sha256(share_pkg);
        let inst_wasm = wasm_sha256(installed_dir);
        match (share_manifest, inst_manifest, share_wasm, inst_wasm) {
            (Some(a), Some(b), Some(sw), Some(iw)) => a != b || sw != iw,
            _ => wasm_fingerprint(share_pkg) != wasm_fingerprint(installed_dir),
        }
    };
    if !need {
        return false;
    }
    let _ = fs::remove_dir_all(installed_dir);
    match copy_dir_recursive(share_pkg, installed_dir) {
        Ok(()) => {
            eprintln!(
                "[aos-session] module synchronisé {} → {}",
                share_pkg.display(),
                installed_dir.display()
            );
            true
        }
        Err(e) => {
            eprintln!(
                "[aos-session] sync module échoué {} : {e}",
                share_pkg.display()
            );
            false
        }
    }
}

pub fn read_version(home: &Path) -> String {
    for p in [home.join("VERSION"), PathBuf::from("VERSION")] {
        if let Ok(s) = fs::read_to_string(p) {
            let t = s.trim().to_string();
            if !t.is_empty() {
                return t;
            }
        }
    }
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg(test)]
mod tests {
    use super::sync_packaged_module;
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("akasha-os-{name}-{}-{nanos}", std::process::id()))
    }

    #[test]
    fn workspace_version_matches_version_file() {
        let file = include_str!("../../../VERSION").trim();
        assert_eq!(
            file,
            env!("CARGO_PKG_VERSION"),
            "VERSION and workspace.package.version must stay in sync"
        );
    }

    #[test]
    fn replace_file_overwrites_existing_dest() {
        let root = temp_dir("replace-file");
        let src = root.join("src.bin");
        let dst = root.join("dst.bin");
        fs::create_dir_all(&root).unwrap();
        fs::write(&src, b"new").unwrap();
        fs::write(&dst, b"old").unwrap();
        super::replace_file(&src, &dst).unwrap();
        assert_eq!(fs::read(&dst).unwrap(), b"new");
        assert!(!root.join("dst.bin.old").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn sync_packaged_module_replaces_outdated_notes_package() {
        let root = temp_dir("sync-packaged-module");
        let share_pkg = root.join("share/modules/notes.aospkg");
        let installed_dir = root.join("var/modules/notes");
        fs::create_dir_all(share_pkg.join("ui")).unwrap();
        fs::create_dir_all(&installed_dir).unwrap();

        fs::write(
            share_pkg.join("manifest.yaml"),
            "name: notes\nhash: new-hash\nversion: 1.1.0\n",
        )
        .unwrap();
        fs::write(share_pkg.join("module.wasm"), b"new wasm").unwrap();
        fs::write(share_pkg.join("ui/index.html"), "new ui").unwrap();

        fs::write(
            installed_dir.join("manifest.yaml"),
            "name: notes\nhash: old-hash\nversion: 1.0.0\n",
        )
        .unwrap();
        fs::write(installed_dir.join("module.wasm"), b"old wasm").unwrap();
        fs::write(installed_dir.join("stale.txt"), "stale").unwrap();

        assert!(sync_packaged_module(&share_pkg, &installed_dir));
        assert_eq!(
            fs::read_to_string(installed_dir.join("manifest.yaml")).unwrap(),
            "name: notes\nhash: new-hash\nversion: 1.1.0\n"
        );
        assert_eq!(
            fs::read(installed_dir.join("module.wasm")).unwrap(),
            b"new wasm"
        );
        assert_eq!(
            fs::read_to_string(installed_dir.join("ui/index.html")).unwrap(),
            "new ui"
        );
        assert!(!installed_dir.join("stale.txt").exists());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ensure_notes_registry_entry_appends_when_tasks_only() {
        let root = temp_dir("ensure-notes-registry");
        let modules = root.join("var/modules");
        let notes = modules.join("notes");
        fs::create_dir_all(&notes).unwrap();
        fs::write(notes.join("module.wasm"), b"notes wasm").unwrap();
        let reg = modules.join("registry.yaml");
        fs::write(
            &reg,
            "installed:\n  - name: tasks\n    granted_caps:\n      - fs.read:/documents/tasks/**\n    quarantined: false\n",
        )
        .unwrap();

        super::ensure_notes_registry_entry(&reg);
        let raw = fs::read_to_string(&reg).unwrap();
        assert!(raw.contains("name: tasks"));
        assert!(raw.contains("name: notes"));
        assert!(raw.contains("mem.query:module:notes"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn headless_bootstrap_retry_dialog_auto_retries() {
        assert!(super::show_bootstrap_retry_dialog(
            "Akasha OS Preview",
            "Couldn't finish starting.",
            "Try again"
        ));
    }

    #[test]
    fn sync_packaged_module_replaces_when_wasm_bytes_differ_same_manifest_hash() {
        let root = temp_dir("sync-packaged-wasm");
        let share_pkg = root.join("share/modules/canvas.aospkg");
        let installed_dir = root.join("var/modules/canvas");
        fs::create_dir_all(share_pkg.join("ui")).unwrap();
        fs::create_dir_all(&installed_dir).unwrap();

        let manifest = "name: canvas\nhash: same-hash\nversion: 1.0.0\n";
        fs::write(share_pkg.join("manifest.yaml"), manifest).unwrap();
        fs::write(share_pkg.join("module.wasm"), b"new wasm with set_style").unwrap();
        fs::write(share_pkg.join("ui/index.html"), "ui").unwrap();

        fs::write(installed_dir.join("manifest.yaml"), manifest).unwrap();
        fs::write(installed_dir.join("module.wasm"), b"old wasm").unwrap();

        assert!(sync_packaged_module(&share_pkg, &installed_dir));
        assert_eq!(
            fs::read(installed_dir.join("module.wasm")).unwrap(),
            b"new wasm with set_style"
        );

        let _ = fs::remove_dir_all(root);
    }
}
