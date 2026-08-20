//! OS helpers: home prefix, folder / browser open, native file picker.

use std::path::{Path, PathBuf};
use eframe::egui;

pub(crate) fn app_icon() -> egui::IconData {
    eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")).expect("app icon")
}

pub(crate) fn request_preview_restart(ctx: &egui::Context) {
    let flag = aos_home().join("var/run/restart_preview.flag");
    if let Some(parent) = flag.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&flag, "ui");
    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
}

pub(crate) fn open_in_browser(url: &str) {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", url])
            .spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(url).spawn();
    }
}

/// Convertit un chemin stocké (`/` ou `\`) en chemin natif.
pub(crate) fn native_path(stored: &str) -> PathBuf {
    PathBuf::from(stored.replace(['/', '\\'], std::path::MAIN_SEPARATOR_STR))
}

/// Ouvre un dossier dans l'explorateur OS.
///
/// Sur Windows, `explorer <chemin>` avec des `/` (ou un dossier absent)
/// ouvre « Mes documents » au lieu de la cible — d'où la normalisation
/// et `/e,chemin`.
pub(crate) fn open_os_folder(dir: &Path) {
    let _ = std::fs::create_dir_all(dir);
    #[cfg(windows)]
    {
        let mut path = dir.to_string_lossy().replace('/', "\\");
        if let Some(stripped) = path.strip_prefix(r"\\?\") {
            path = stripped.to_string();
        }
        let _ = std::process::Command::new("explorer")
            .arg(format!("/e,{path}"))
            .spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(dir).spawn();
    }
}

/// Native OS file dialog. Returns `None` if the user cancels.
pub(crate) fn pick_os_file(
    title: &str,
    filters: &[(&str, &[&str])],
    start_dir: Option<&Path>,
) -> Option<PathBuf> {
    let mut dlg = rfd::FileDialog::new().set_title(title);
    for (name, exts) in filters {
        dlg = dlg.add_filter(*name, exts);
    }
    if let Some(dir) = start_dir.filter(|p| p.is_dir()) {
        dlg = dlg.set_directory(dir);
    }
    dlg.pick_file()
}

pub(crate) fn user_downloads_dir() -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    let dir = PathBuf::from(home).join("Downloads");
    dir.is_dir().then_some(dir)
}

pub(crate) fn open_url(url: &str) {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(url).spawn();
    }
}

pub(crate) fn aos_home() -> PathBuf {
    std::env::var("AOS_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

pub(crate) fn bin_aos_session() -> PathBuf {
    let exe = if cfg!(windows) {
        "aos-session.exe"
    } else {
        "aos-session"
    };
    let p = aos_home().join("bin").join(exe);
    if p.exists() {
        p
    } else {
        PathBuf::from(exe)
    }
}
