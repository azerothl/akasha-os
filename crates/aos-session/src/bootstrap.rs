//! Premier run : disque, NVIDIA, modèles GGUF, skills.

use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

const MIN_FREE_BYTES: u64 = 4 * 1024 * 1024 * 1024; // ~4 Go

#[derive(Debug, Deserialize)]
struct ModelsManifest {
    #[allow(dead_code)]
    version: Option<String>,
    models: Vec<ModelEntry>,
}

#[derive(Debug, Deserialize)]
struct ModelEntry {
    filename: String,
    #[allow(dead_code)]
    role: Option<String>,
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
                    "espace disque insuffisant (~{:.1} Go libres, ~4 Go requis)",
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
                            "espace disque insuffisant (~{:.1} Go libres, ~4 Go requis)",
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
    let already = fs::read_dir(&dest)
        .map(|rd| rd.filter_map(|e| e.ok()).any(|e| e.path().is_dir()))
        .unwrap_or(false);
    if already {
        return;
    }
    for cand in [
        home.join("share/skills"),
        PathBuf::from("share/skills"),
        PathBuf::from("skills"),
    ] {
        if cand.is_dir() {
            let _ = copy_dir_recursive(&cand, &dest);
            eprintln!("[aos-session] skills copiés depuis {}", cand.display());
            return;
        }
    }
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
        download_file(&m.url, &path, m.bytes)?;
        if !m.sha256.is_empty() {
            let got = file_sha256(&path)?;
            if !got.eq_ignore_ascii_case(&m.sha256) {
                let _ = fs::remove_file(&path);
                return Err(format!(
                    "sha256 invalide pour {} (attendu {}, obtenu {got})",
                    m.filename, m.sha256
                ));
            }
        }
        eprintln!("[aos-session] téléchargé {}", m.filename);
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
    let total = expected.or_else(|| {
        resp.content_length()
            .map(|l| l + resume_from)
    });
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
            if t > 0 {
                let pct = done * 100 / t;
                if pct >= last_pct + 5 {
                    eprintln!("[aos-session]   {pct}% ({done}/{t})");
                    last_pct = pct;
                }
            }
        }
    }
    drop(file);
    fs::rename(&tmp, dest).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &to)?;
        } else {
            fs::copy(entry.path(), to)?;
        }
    }
    Ok(())
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
