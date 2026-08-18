//! Fetch sd.cpp / piper into `bin/` when a media pack is installed.
//!
//! Weights stay optional (P08.6). The matching **engine zip** is not in the
//! Preview artefact — it is downloaded the first time the pack is.

use crate::bootstrap;
use crate::offerings::{EngineArtifact, OfferingsFile};
use crate::update;
use std::fs;
use std::path::{Path, PathBuf};

pub fn engine_id_for(
    offering_engine: Option<&str>,
    profiles: &[String],
    modality: Option<&str>,
) -> Option<String> {
    if let Some(id) = offering_engine.filter(|s| !s.is_empty()) {
        return Some(id.to_string());
    }
    if profiles.iter().any(|p| p == "image") || modality == Some("image") {
        return Some("sdcpp".into());
    }
    if profiles.iter().any(|p| p == "tts") || modality == Some("audio") {
        return Some("piper".into());
    }
    None
}

pub fn markers_present(bin: &Path, engine_id: &str) -> bool {
    match engine_id {
        "piper" => exe_exists(bin, "piper"),
        "sdcpp" => exe_exists(bin, "sd") || exe_exists(bin, "sd-cli"),
        _ => false,
    }
}

fn exe_exists(dir: &Path, name: &str) -> bool {
    if cfg!(windows) {
        dir.join(format!("{name}.exe")).is_file()
    } else {
        dir.join(name).is_file()
    }
}

pub fn ensure_engine(
    home: &Path,
    offerings: Option<&OfferingsFile>,
    engine_id: &str,
) -> Result<(), String> {
    let bin = home.join("bin");
    fs::create_dir_all(&bin).map_err(|e| e.to_string())?;
    if markers_present(&bin, engine_id) {
        return Ok(());
    }
    let artifact = artifact_for(offerings, engine_id)?;
    eprintln!("[aos-session] moteur {engine_id} manquant — téléchargement…");
    let cache = home.join("var/cache/engines");
    fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let archive = cache.join(&artifact.filename);
    bootstrap::download_model_file(
        &artifact.url,
        &archive,
        Some(artifact.bytes).filter(|b| *b > 0),
        &artifact.sha256,
    )?;
    let extract = cache.join(format!("{engine_id}-extract"));
    let _ = fs::remove_dir_all(&extract);
    fs::create_dir_all(&extract).map_err(|e| e.to_string())?;
    extract_archive(&archive, &extract)?;
    install_from_extract(&extract, &bin, engine_id)?;
    if !markers_present(&bin, engine_id) {
        return Err(format!(
            "moteur {engine_id} : binaire introuvable après extraction"
        ));
    }
    Ok(())
}

/// Best-effort: if a media pack is already installed, fetch its engine.
pub fn repair_installed(home: &Path) {
    let offerings = crate::offerings::load_offerings(home).ok();
    let inst = crate::offerings::load_installed(home);
    let mut needed: Vec<String> = Vec::new();
    for m in &inst.models {
        let engine = if let Some(off) = offerings
            .as_ref()
            .and_then(|o| crate::offerings::find_offering(o, &m.id))
        {
            engine_id_for(off.engine.as_deref(), &off.profiles, off.modality.as_deref())
        } else {
            engine_id_for(None, &m.profiles, None)
        };
        if let Some(id) = engine {
            if !needed.iter().any(|x| x == &id) {
                needed.push(id);
            }
        }
    }
    for id in needed {
        if let Err(e) = ensure_engine(home, offerings.as_ref(), &id) {
            eprintln!("[aos-session] moteur {id} : {e}");
        }
    }
}

fn artifact_for(
    offerings: Option<&OfferingsFile>,
    engine_id: &str,
) -> Result<EngineArtifact, String> {
    if let Some(off) = offerings {
        if let Some(e) = off.engines.get(engine_id) {
            if let Some(a) = e.current_os() {
                return Ok(a.clone());
            }
        }
    }
    builtin(engine_id).ok_or_else(|| {
        format!("pas d'artefact moteur pour {engine_id} sur cet OS")
    })
}

fn builtin(engine_id: &str) -> Option<EngineArtifact> {
    match engine_id {
        "piper" if cfg!(windows) => Some(EngineArtifact {
            url: "https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_windows_amd64.zip".into(),
            filename: "piper_windows_amd64.zip".into(),
            bytes: 22_477_236,
            sha256: String::new(),
        }),
        "piper" if cfg!(target_os = "linux") => Some(EngineArtifact {
            url: "https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_linux_x86_64.tar.gz".into(),
            filename: "piper_linux_x86_64.tar.gz".into(),
            bytes: 25_200_000,
            sha256: String::new(),
        }),
        "sdcpp" if cfg!(windows) => Some(EngineArtifact {
            url: "https://github.com/leejet/stable-diffusion.cpp/releases/download/master-820-de298c2/sd-master-de298c2-bin-win-vulkan-x64.zip".into(),
            filename: "sd-master-de298c2-bin-win-vulkan-x64.zip".into(),
            bytes: 0,
            sha256: String::new(),
        }),
        "sdcpp" if cfg!(target_os = "linux") => Some(EngineArtifact {
            url: "https://github.com/leejet/stable-diffusion.cpp/releases/download/master-820-de298c2/sd-master-de298c2-bin-Linux-Ubuntu-24.04-x86_64-vulkan.zip".into(),
            filename: "sd-master-de298c2-bin-Linux-Ubuntu-24.04-x86_64-vulkan.zip".into(),
            bytes: 0,
            sha256: String::new(),
        }),
        _ => None,
    }
}

fn extract_archive(archive: &Path, dest: &Path) -> Result<(), String> {
    let name = archive
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if name.ends_with(".zip") {
        update::extract_zip(archive, dest)
    } else {
        update::extract_tar_gz(archive, dest)
    }
}

fn install_from_extract(extract: &Path, bin: &Path, engine_id: &str) -> Result<(), String> {
    fs::create_dir_all(bin).map_err(|e| e.to_string())?;
    copy_runtime_files(extract, bin)?;
    if engine_id == "sdcpp" {
        let sd_cli = exe_path(bin, "sd-cli");
        let sd = exe_path(bin, "sd");
        if sd_cli.is_file() && !sd.is_file() {
            fs::copy(&sd_cli, &sd).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn exe_path(dir: &Path, name: &str) -> PathBuf {
    if cfg!(windows) {
        dir.join(format!("{name}.exe"))
    } else {
        dir.join(name)
    }
}

fn copy_runtime_files(from: &Path, bin: &Path) -> Result<(), String> {
    for entry in fs::read_dir(from).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if path.is_dir() {
            if name_str.eq_ignore_ascii_case("espeak-ng-data") {
                let dest = bin.join("espeak-ng-data");
                let _ = fs::remove_dir_all(&dest);
                bootstrap::copy_dir_recursive(&path, &dest).map_err(|e| e.to_string())?;
            } else if name_str.eq_ignore_ascii_case("pkgconfig") {
                continue;
            } else {
                copy_runtime_files(&path, bin)?;
            }
        } else if keep_runtime_file(&name_str) {
            fs::copy(&path, bin.join(&name)).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn keep_runtime_file(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".txt") || lower.ends_with(".md") {
        return false;
    }
    let ext = Path::new(&lower)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    matches!(ext, "exe" | "dll" | "ort" | "so" | "dylib")
        || matches!(lower.as_str(), "piper" | "sd" | "sd-cli")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "akasha-os-engines-{}-{}-{nanos}",
            name,
            std::process::id()
        ))
    }

    fn exe_name(name: &str) -> String {
        if cfg!(windows) {
            format!("{name}.exe")
        } else {
            name.to_string()
        }
    }

    #[test]
    fn engine_id_from_profiles() {
        assert_eq!(
            engine_id_for(None, &["image".into()], None).as_deref(),
            Some("sdcpp")
        );
        assert_eq!(
            engine_id_for(None, &["tts".into()], None).as_deref(),
            Some("piper")
        );
        assert_eq!(
            engine_id_for(Some("sdcpp"), &["image".into()], None).as_deref(),
            Some("sdcpp")
        );
        assert_eq!(engine_id_for(None, &["chat".into()], None), None);
    }

    #[test]
    fn install_piper_layout_into_bin() {
        let root = temp_dir("piper");
        let extract = root.join("extract/piper");
        let bin = root.join("bin");
        fs::create_dir_all(extract.join("espeak-ng-data")).unwrap();
        fs::write(extract.join(exe_name("piper")), b"piper").unwrap();
        fs::write(extract.join("espeak-ng.dll"), b"dll").unwrap();
        fs::write(extract.join("espeak-ng-data/fr_dict"), b"fr").unwrap();
        fs::write(extract.join("readme.txt"), b"skip").unwrap();

        install_from_extract(&root.join("extract"), &bin, "piper").unwrap();
        assert!(exe_path(&bin, "piper").is_file());
        assert!(bin.join("espeak-ng.dll").is_file());
        assert!(bin.join("espeak-ng-data/fr_dict").is_file());
        assert!(!bin.join("readme.txt").is_file());
        assert!(markers_present(&bin, "piper"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn install_sd_cli_renames_to_sd() {
        let root = temp_dir("sd");
        let extract = root.join("extract");
        let bin = root.join("bin");
        fs::create_dir_all(&extract).unwrap();
        fs::write(extract.join(exe_name("sd-cli")), b"sd").unwrap();
        fs::write(extract.join("ggml.dll"), b"dll").unwrap();

        install_from_extract(&extract, &bin, "sdcpp").unwrap();
        assert!(exe_path(&bin, "sd-cli").is_file());
        assert!(exe_path(&bin, "sd").is_file());
        assert!(bin.join("ggml.dll").is_file());
        assert!(markers_present(&bin, "sdcpp"));
        let _ = fs::remove_dir_all(root);
    }
}
