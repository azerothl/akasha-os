//! Mises à jour non destructives via GitHub Releases.

use crate::bootstrap::copy_dir_recursive;
use serde::Deserialize;
use std::fs::{self, File};
use std::io::copy;
use std::path::{Path, PathBuf};

const GITHUB_REPO: &str = "azerothl/akasha-os";

#[derive(Debug, Deserialize)]
struct GhRelease {
    tag_name: String,
    html_url: String,
    assets: Vec<GhAsset>,
}

#[derive(Debug, Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UpdateInfo {
    pub version: String,
    pub tag: String,
    pub html_url: String,
    pub asset_name: String,
    pub download_url: String,
    pub size: u64,
}

/// Compare semver-like `a` vs `b`. Retourne true si `remote` > `local`.
pub fn is_newer(local: &str, remote: &str) -> bool {
    let parse = |s: &str| -> Vec<u64> {
        s.trim()
            .trim_start_matches('v')
            .split(|c| c == '.' || c == '-')
            .filter_map(|p| {
                p.chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse()
                    .ok()
            })
            .collect()
    };
    let a = parse(local);
    let b = parse(remote);
    let n = a.len().max(b.len());
    for i in 0..n {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        if y > x {
            return true;
        }
        if y < x {
            return false;
        }
    }
    false
}

pub fn check_latest(local_version: &str) -> Result<Option<UpdateInfo>, String> {
    let url = format!("https://api.github.com/repos/{GITHUB_REPO}/releases/latest");
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("akasha-os-preview")
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client.get(&url).send().map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("GitHub API {}", resp.status()));
    }
    let rel: GhRelease = resp.json().map_err(|e| e.to_string())?;
    let tag = rel.tag_name.clone();
    let ver = tag.trim_start_matches('v').to_string();
    if !is_newer(local_version, &ver) {
        return Ok(None);
    }
    let want = if cfg!(windows) {
        format!("AgentOS-Preview-{ver}-windows-x64.zip")
    } else {
        format!("AgentOS-Preview-{ver}-linux-x64.tar.gz")
    };
    let asset = rel
        .assets
        .iter()
        .find(|a| a.name == want)
        .or_else(|| {
            rel.assets.iter().find(|a| {
                if cfg!(windows) {
                    a.name.contains("windows") && a.name.ends_with(".zip")
                } else {
                    a.name.contains("linux") && a.name.ends_with(".tar.gz")
                }
            })
        })
        .ok_or_else(|| format!("asset {want} introuvable dans la release"))?;
    Ok(Some(UpdateInfo {
        version: ver,
        tag,
        html_url: rel.html_url,
        asset_name: asset.name.clone(),
        download_url: asset.browser_download_url.clone(),
        size: asset.size,
    }))
}

/// Télécharge l'archive dans `var/updates/staging/` et marque apply-on-next-boot.
pub fn download_update(home: &Path, info: &UpdateInfo) -> Result<PathBuf, String> {
    let staging = home.join("var/updates/staging");
    fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    let archive = staging.join(&info.asset_name);
    eprintln!(
        "[aos-session] téléchargement update {} (~{:.1} MiB)…",
        info.asset_name,
        info.size as f64 / (1 << 20) as f64
    );
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .user_agent("akasha-os-preview")
        .build()
        .map_err(|e| e.to_string())?;
    let mut resp = client
        .get(&info.download_url)
        .send()
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let mut file = File::create(&archive).map_err(|e| e.to_string())?;
    copy(&mut resp, &mut file).map_err(|e| e.to_string())?;
    let pending = home.join("var/updates/pending.json");
    let doc = serde_json::json!({
        "version": info.version,
        "archive": archive.to_string_lossy(),
        "asset_name": info.asset_name,
    });
    fs::write(&pending, serde_json::to_string_pretty(&doc).unwrap()).map_err(|e| e.to_string())?;
    Ok(archive)
}

/// Applique une mise à jour en attente (appelé au tout début de aos-session).
pub fn apply_pending_update(home: &Path) -> Result<bool, String> {
    let pending = home.join("var/updates/pending.json");
    if !pending.exists() {
        return Ok(false);
    }
    let raw = fs::read_to_string(&pending).map_err(|e| e.to_string())?;
    let v: serde_json::Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    let archive = PathBuf::from(v.get("archive").and_then(|x| x.as_str()).unwrap_or(""));
    if !archive.exists() {
        let _ = fs::remove_file(&pending);
        return Err(format!("archive absente: {}", archive.display()));
    }
    eprintln!("[aos-session] application update depuis {}", archive.display());
    let extract = home.join("var/updates/extract");
    let _ = fs::remove_dir_all(&extract);
    fs::create_dir_all(&extract).map_err(|e| e.to_string())?;

    if archive
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.ends_with(".zip"))
        .unwrap_or(false)
    {
        extract_zip(&archive, &extract)?;
    } else {
        extract_tar_gz(&archive, &extract)?;
    }

    let root = find_package_root(&extract)?;
    overlay_program(home, &root)?;

    let _ = fs::remove_file(&pending);
    let _ = fs::remove_dir_all(&extract);
    eprintln!("[aos-session] update appliquée (var/ et etc/ préservés)");
    Ok(true)
}

fn find_package_root(extract: &Path) -> Result<PathBuf, String> {
    if extract.join("bin").is_dir() {
        return Ok(extract.to_path_buf());
    }
    for entry in fs::read_dir(extract).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.path().join("bin").is_dir() {
            return Ok(entry.path());
        }
    }
    Err("structure de paquet invalide (pas de bin/)".into())
}

/// Overlay `bin/` / `share/` / `data/` / docs / VERSION onto `home` without
/// deleting `var/` or overwriting existing `etc/*.yaml` (writes `.new` instead).
pub fn overlay_program(home: &Path, pkg: &Path) -> Result<(), String> {
    for dir in ["bin", "share", "data", "docs"] {
        let src = pkg.join(dir);
        if !src.is_dir() {
            continue;
        }
        if dir == "share" {
            overlay_share(home, &src)?;
            continue;
        }
        let dst = home.join(dir);
        fs::create_dir_all(&dst).map_err(|e| e.to_string())?;
        copy_dir_recursive(&src, &dst).map_err(|e| e.to_string())?;
    }
    for f in [
        "VERSION",
        "INSTALL.md",
        "TESTER.md",
        "FIRST-RUN.md",
        "README.txt",
        "LICENSE",
        "NOTICE",
        "LICENSE-COMMERCIAL.md",
    ] {
        let src = pkg.join(f);
        if src.exists() {
            let _ = fs::copy(&src, home.join(f));
        }
    }
    let etc_src = pkg.join("etc");
    if etc_src.is_dir() {
        let etc_dst = home.join("etc");
        fs::create_dir_all(&etc_dst).map_err(|e| e.to_string())?;
        for entry in fs::read_dir(&etc_src).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                continue;
            }
            let name = entry.file_name();
            let dst = etc_dst.join(&name);
            if dst.exists() {
                let _ = fs::copy(
                    entry.path(),
                    etc_dst.join(format!("{}.new", name.to_string_lossy())),
                );
            } else {
                let _ = fs::copy(entry.path(), dst);
            }
        }
    }
    Ok(())
}

fn overlay_share(home: &Path, share_src: &Path) -> Result<(), String> {
    let share_dst = home.join("share");
    fs::create_dir_all(&share_dst).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(share_src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let name_s = name.to_string_lossy();
        let dst = share_dst.join(&name);
        if name_s == "models" {
            fs::create_dir_all(&dst).map_err(|e| e.to_string())?;
            for m in fs::read_dir(entry.path()).map_err(|e| e.to_string())? {
                let m = m.map_err(|e| e.to_string())?;
                let fname = m.file_name();
                let fs_name = fname.to_string_lossy();
                let to = dst.join(&fname);
                if fs_name.ends_with(".gguf") && to.exists() {
                    continue;
                }
                if m.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    copy_dir_recursive(&m.path(), &to).map_err(|e| e.to_string())?;
                } else {
                    let _ = fs::copy(m.path(), to);
                }
            }
        } else if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            let _ = fs::remove_dir_all(&dst);
            copy_dir_recursive(&entry.path(), &dst).map_err(|e| e.to_string())?;
        } else {
            let _ = fs::copy(entry.path(), dst);
        }
    }
    Ok(())
}

pub(crate) fn extract_zip(archive: &Path, dest: &Path) -> Result<(), String> {
    let file = File::open(archive).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    for i in 0..zip.len() {
        let mut file = zip.by_index(i).map_err(|e| e.to_string())?;
        let outpath = match file.enclosed_name() {
            Some(p) => dest.join(p),
            None => continue,
        };
        if file.name().ends_with('/') {
            fs::create_dir_all(&outpath).map_err(|e| e.to_string())?;
        } else {
            if let Some(p) = outpath.parent() {
                fs::create_dir_all(p).map_err(|e| e.to_string())?;
            }
            let mut outfile = File::create(&outpath).map_err(|e| e.to_string())?;
            copy(&mut file, &mut outfile).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub(crate) fn extract_tar_gz(archive: &Path, dest: &Path) -> Result<(), String> {
    let status = std::process::Command::new("tar")
        .args([
            "-xzf",
            archive.to_str().unwrap_or(""),
            "-C",
            dest.to_str().unwrap_or("."),
        ])
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("tar extract échoué".into());
    }
    Ok(())
}

pub fn write_update_offer(home: &Path, info: &UpdateInfo) -> Result<(), String> {
    let path = home.join("var/run/update_available.json");
    fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    fs::write(path, serde_json::to_string_pretty(info).unwrap()).map_err(|e| e.to_string())
}

pub fn clear_update_offer(home: &Path) {
    let _ = fs::remove_file(home.join("var/run/update_available.json"));
}

pub fn read_update_offer(home: &Path) -> Option<UpdateInfo> {
    let raw = fs::read_to_string(home.join("var/run/update_available.json")).ok()?;
    serde_json::from_str(&raw).ok()
}
