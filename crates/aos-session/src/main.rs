//! `aos-session` — superviseur Preview.
//!
//! Remplace `run-demo.ps1` pour les testeurs : résout `AOS_HOME`, génère les
//! configs relatives, démarre les daemons, lance egui, arrête proprement.

mod bootstrap;
mod engines;
mod hardware;
mod offerings;
mod update;

use aos_ipc::BusClient;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const BUS_ADDR: &str = "127.0.0.1:24701";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct OnboardingState {
    completed: bool,
    language: String,
    routing: String,
    trust_default: String,
    #[serde(default)]
    tutorial_step: u32,
}

struct Daemon {
    name: &'static str,
    child: Child,
}

struct Session {
    home: PathBuf,
    daemons: Mutex<Vec<Daemon>>,
    stop: AtomicBool,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--download-update") {
        let home = resolve_home();
        std::env::set_var("AOS_HOME", &home);
        let _ = std::env::set_current_dir(&home);
        match update::read_update_offer(&home) {
            Some(info) => {
                if let Some(p) = update::pending_archive_for(&home, &info.version) {
                    eprintln!(
                        "[aos-session] update déjà en staging — redémarrez pour appliquer ({})",
                        p.display()
                    );
                    std::process::exit(0);
                }
                match update::download_update(&home, &info) {
                    Ok(p) => {
                        eprintln!(
                            "[aos-session] update téléchargée — redémarrez pour appliquer ({})",
                            p.display()
                        );
                        std::process::exit(0);
                    }
                    Err(e) => {
                        eprintln!("[aos-session] download update échoué : {e}");
                        std::process::exit(1);
                    }
                }
            }
            None => {
                eprintln!(
                    "[aos-session] aucune offre de mise à jour (var/run/update_available.json)"
                );
                std::process::exit(1);
            }
        }
    }

    if let Some(pos) = args.iter().position(|a| a == "--remove-model") {
        let home = resolve_home();
        std::env::set_var("AOS_HOME", &home);
        let _ = std::env::set_current_dir(&home);
        let id = args.get(pos + 1).map(String::as_str).unwrap_or("");
        if id.is_empty() {
            eprintln!("[aos-session] usage: aos-session --remove-model <id>");
            std::process::exit(1);
        }
        match offerings::remove_installed_model(&home, id) {
            Ok(()) => {
                std::env::set_var("AOS_FORCE_CONFIG", "1");
                write_runtime_configs(&home);
                eprintln!("[aos-session] modèle supprimé ({id}) — redémarrez Preview");
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("[aos-session] remove model : {e}");
                std::process::exit(1);
            }
        }
    }

    if let Some(pos) = args.iter().position(|a| a == "--redownload-models") {
        let home = resolve_home();
        std::env::set_var("AOS_HOME", &home);
        let _ = std::env::set_current_dir(&home);
        let ids: Vec<String> = args[pos + 1..].to_vec();
        if ids.is_empty() {
            eprintln!("[aos-session] usage: aos-session --redownload-models <id>…");
            std::process::exit(1);
        }
        match offerings::redownload_ids(&home, &ids) {
            Ok(()) => {
                std::env::set_var("AOS_FORCE_CONFIG", "1");
                write_runtime_configs(&home);
                eprintln!("[aos-session] modèles re-téléchargés — redémarrez Preview");
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("[aos-session] redownload models : {e}");
                std::process::exit(1);
            }
        }
    }

    if let Some(pos) = args.iter().position(|a| a == "--download-hf-url") {
        let home = resolve_home();
        std::env::set_var("AOS_HOME", &home);
        let _ = std::env::set_current_dir(&home);
        let url = args.get(pos + 1).map(String::as_str).unwrap_or("");
        if url.is_empty() {
            eprintln!("[aos-session] usage: aos-session --download-hf-url <resolve-url> [--name <display>]");
            std::process::exit(1);
        }
        let name = args
            .iter()
            .position(|a| a == "--name")
            .and_then(|i| args.get(i + 1))
            .map(String::as_str);
        match offerings::download_hf_url(&home, url, name) {
            Ok(id) => {
                std::env::set_var("AOS_FORCE_CONFIG", "1");
                write_runtime_configs(&home);
                eprintln!("[aos-session] modèle HF téléchargé ({id}) — redémarrez Preview");
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("[aos-session] download HF : {e}");
                std::process::exit(1);
            }
        }
    }

    if let Some(pos) = args.iter().position(|a| a == "--download-models") {
        let home = resolve_home();
        std::env::set_var("AOS_HOME", &home);
        let _ = std::env::set_current_dir(&home);
        let ids: Vec<String> = args[pos + 1..].to_vec();
        if ids.is_empty() {
            eprintln!("[aos-session] usage: aos-session --download-models <id>…");
            std::process::exit(1);
        }
        match offerings::download_ids(&home, &ids) {
            Ok(()) => {
                std::env::set_var("AOS_FORCE_CONFIG", "1");
                write_runtime_configs(&home);
                eprintln!("[aos-session] modèles téléchargés — redémarrez Preview");
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("[aos-session] download models : {e}");
                std::process::exit(1);
            }
        }
    }

    let home = resolve_home();
    std::env::set_var("AOS_HOME", &home);
    std::env::set_current_dir(&home).expect("chdir AOS_HOME");

    let version = bootstrap::read_version(&home);
    eprintln!("[aos-session] Akasha OS Preview {version}");
    eprintln!("[aos-session] AOS_HOME={}", home.display());

    // Appliquer une update téléchargée avant de toucher aux binaires en cours.
    match update::apply_pending_update(&home) {
        Ok(true) => {
            eprintln!("[aos-session] update appliquée — relance du superviseur");
            reexec_updated(&home);
        }
        Ok(false) => {}
        Err(e) => {
            eprintln!("[aos-session] apply update : {e}");
            eprintln!(
                "[aos-session] l'archive reste en attente — fermez toutes les fenêtres Preview puis relancez"
            );
        }
    }

    ensure_layout(&home);
    bootstrap::ensure_skills(&home);
    bootstrap::ensure_mcp_stub(&home);

    if let Err(e) = bootstrap::check_disk_space(&home) {
        bootstrap::show_fatal_dialog(
            "Akasha OS Preview — disque",
            &format!("{e}\n\nLibérez de l'espace puis relancez. Voir FIRST-RUN.md."),
        );
        std::process::exit(3);
    }

    let nvidia = bootstrap::nvidia_ok();
    // Preferences may request CPU inference (Settings → Inference).
    let prefs_cpu = std::fs::read_to_string(home.join("var/run/preferences.json"))
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .and_then(|v| {
            v.get("inference_mode")
                .and_then(|m| m.as_str())
                .map(|m| m.eq_ignore_ascii_case("cpu"))
        })
        .unwrap_or(false);
    let force_cpu = prefs_cpu
        || std::env::var_os("AOS_CPU_ONLY").is_some()
        || std::env::var("AOS_INFERENCE")
            .map(|v| v.eq_ignore_ascii_case("cpu"))
            .unwrap_or(false);
    let require_gpu = std::env::var_os("AOS_REQUIRE_GPU").is_some()
        || std::fs::read_to_string(home.join("var/run/preferences.json"))
            .ok()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
            .and_then(|v| {
                v.get("inference_mode")
                    .and_then(|m| m.as_str())
                    .map(|m| m.eq_ignore_ascii_case("gpu"))
            })
            .unwrap_or(false);
    if !nvidia && !force_cpu && require_gpu {
        bootstrap::show_fatal_dialog(
            "Akasha OS Preview — GPU NVIDIA requis",
            "nvidia-smi introuvable ou en échec.\n\
             AOS_REQUIRE_GPU est défini — pas de fallback CPU.\n\
             Installez un driver NVIDIA, ou relancez avec AOS_CPU_ONLY=1.\n\
             Voir INSTALL.md / FIRST-RUN.md.",
        );
        std::process::exit(2);
    }
    if !nvidia || force_cpu {
        eprintln!(
            "[aos-session] mode CPU-only (nvidia_ok={}, force_cpu={})",
            nvidia, force_cpu
        );
        std::env::set_var("AOS_CPU_ONLY", "1");
        if !nvidia && !force_cpu {
            eprintln!(
                "[aos-session] NVIDIA absent — démarrage CPU (lent). \
                 Définir AOS_REQUIRE_GPU=1 pour refuser."
            );
        }
    }

    let hw = hardware::probe(&home);
    if let Err(e) = hw.save(&home) {
        eprintln!("[aos-session] hardware.json : {e}");
    }
    eprintln!(
        "[aos-session] hardware: {} — {} MiB VRAM — tier {}",
        hw.gpu_name,
        hw.vram_mib,
        hw.tier.as_str()
    );
    if let Some(ref bw) = hw.bandwidth {
        eprintln!(
            "[aos-session] bandwidth: RAM {:.1} GB/s ({:?})",
            bw.ram_mem_bw.bytes_per_sec / 1e9,
            bw.ram_mem_bw.source
        );
        if let Some(ref g) = bw.gpu_mem_bw {
            eprintln!(
                "[aos-session] bandwidth: GPU {:.1} GB/s ({:?}) — {}",
                g.bytes_per_sec / 1e9,
                g.source,
                g.detail
            );
        }
        if let Some(ref h) = bw.host_to_device_bw {
            eprintln!(
                "[aos-session] bandwidth: H2D {:.1} GB/s ({:?}) — {}",
                h.bytes_per_sec / 1e9,
                h.source,
                h.detail
            );
        }
    }

    let _ = offerings::migrate_legacy_installed(&home);

    if offerings::setup_needed(&home) {
        if let Err(e) = run_model_setup(&home, &hw, &version) {
            bootstrap::show_fatal_dialog(
                "Akasha OS Preview — modèles",
                &format!(
                    "Configuration des modèles annulée ou échouée :\n{e}\n\n\
                     Voir share/models/catalog-offerings.json et FIRST-RUN.md."
                ),
            );
            std::process::exit(4);
        }
    } else if let Err(e) = ensure_installed_files_present(&home) {
        bootstrap::show_fatal_dialog(
            "Akasha OS Preview — modèles",
            &format!("Modèles installés incomplets :\n{e}"),
        );
        std::process::exit(4);
    }
    engines::repair_installed(&home);

    let _ = offerings::detect_model_updates(&home, &hw);

    // Toujours régénérer les configs avec VRAM réelle + modèles installés.
    std::env::set_var("AOS_FORCE_CONFIG", "1");
    write_runtime_configs(&home);
    std::env::remove_var("AOS_FORCE_CONFIG");

    let session = Arc::new(Session {
        home: home.clone(),
        daemons: Mutex::new(Vec::new()),
        stop: AtomicBool::new(false),
    });

    {
        let s = session.clone();
        ctrlc_guard(s);
    }

    // Check updates en arrière-plan (best-effort, repo public).
    {
        let home_bg = home.clone();
        let ver_bg = version.clone();
        thread::spawn(move || match update::check_latest(&ver_bg) {
            Ok(Some(info)) => {
                let _ = update::write_update_offer(&home_bg, &info);
                eprintln!(
                    "[aos-session] mise à jour disponible : {} ({})",
                    info.version, info.html_url
                );
                let auto = std::fs::read_to_string(home_bg.join("var/run/preferences.json"))
                    .ok()
                    .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
                    .and_then(|v| v.get("auto_download_updates")?.as_bool())
                    .unwrap_or(false);
                if auto {
                    if let Some(p) = update::pending_archive_for(&home_bg, &info.version) {
                        eprintln!(
                            "[aos-session] update déjà en staging — redémarrez pour appliquer ({})",
                            p.display()
                        );
                    } else {
                        match update::download_update(&home_bg, &info) {
                                Ok(p) => eprintln!(
                                    "[aos-session] update auto-téléchargée — redémarrez pour appliquer ({})",
                                    p.display()
                                ),
                                Err(e) => eprintln!("[aos-session] auto-download update : {e}"),
                            }
                    }
                }
            }
            Ok(None) => update::clear_update_offer(&home_bg),
            Err(e) => eprintln!("[aos-session] check update : {e}"),
        });
    }

    {
        let s = session.clone();
        thread::spawn(move || auditd_watchdog(s));
    }
    {
        let s = session.clone();
        thread::spawn(move || platformd_watchdog(s));
    }
    {
        let s = session.clone();
        thread::spawn(move || modeld_watchdog(s));
    }

    let restart_flag = home.join("var/run/restart_preview.flag");
    let mut first_ui = true;
    loop {
        session.stop.store(false, Ordering::SeqCst);

        if let Err(e) = start_daemons(&session) {
            eprintln!("[aos-session] démarrage échoué : {e}");
            eprintln!("[aos-session] Astuce : consultez var/run/*.stderr.log (GPU, modèles, bus).");
            stop_all(&session);
            std::process::exit(1);
        }

        if let Err(e) = healthcheck() {
            eprintln!("[aos-session] healthcheck échoué : {e}");
            eprintln!(
                "[aos-session] Causes fréquentes : modèle GGUF manquant, CUDA DLL absente, bus occupé.\n\
                 Logs : var/run/*.stderr.log"
            );
            stop_all(&session);
            std::process::exit(1);
        }
        eprintln!("[aos-session] services OK");
        if first_ui {
            apply_trust_default(&home);
            first_ui = false;
        }

        let ui = bin_path(&home, "aos-ui-egui");
        let mut ui_cmd = Command::new(&ui);
        ui_cmd
            .env("AOS_HOME", &home)
            .env("AOS_PREVIEW_VERSION", &version)
            .current_dir(&home);
        #[cfg(target_os = "linux")]
        {
            let bin_dir = home.join("bin");
            let mut ld = bin_dir.to_string_lossy().to_string();
            if let Ok(prev) = std::env::var("LD_LIBRARY_PATH") {
                if !prev.is_empty() {
                    ld = format!("{ld}:{prev}");
                }
            }
            ui_cmd.env("LD_LIBRARY_PATH", &ld);
        }
        let status = ui_cmd.status();

        let restart = restart_flag.exists();
        if restart {
            let _ = fs::remove_file(&restart_flag);
            std::env::set_var("AOS_FORCE_CONFIG", "1");
            write_runtime_configs(&home);
            std::env::remove_var("AOS_FORCE_CONFIG");
        }

        session.stop.store(true, Ordering::SeqCst);
        stop_all(&session);

        if restart {
            eprintln!("[aos-session] redémarrage Preview demandé par l'UI…");
            thread::sleep(Duration::from_millis(400));
            continue;
        }

        match status {
            Ok(st) if st.success() => {}
            Ok(st) => {
                eprintln!("[aos-session] UI exit {st}");
                std::process::exit(st.code().unwrap_or(1));
            }
            Err(e) => {
                eprintln!("[aos-session] UI failed: {e}");
                std::process::exit(1);
            }
        }
        break;
    }
}

fn run_model_setup(home: &Path, hw: &hardware::HardwareInfo, version: &str) -> Result<(), String> {
    let offer = offerings::build_setup_offer(home, hw)?;
    offerings::write_setup_offer(home, &offer)?;
    eprintln!(
        "[aos-session] model setup — recommandé : {:?}",
        offer.recommended_ids
    );

    // Prefer egui confirmation UI; fall back to auto-best if UI missing.
    let ui = bin_path(home, "aos-ui-egui");
    if ui.exists() {
        let mut ui_cmd = Command::new(&ui);
        ui_cmd
            .env("AOS_HOME", home)
            .env("AOS_PREVIEW_VERSION", version)
            .env("AOS_MODEL_SETUP", "1")
            .current_dir(home);
        #[cfg(target_os = "linux")]
        {
            let bin_dir = home.join("bin");
            let mut ld = bin_dir.to_string_lossy().to_string();
            if let Ok(prev) = std::env::var("LD_LIBRARY_PATH") {
                if !prev.is_empty() {
                    ld = format!("{ld}:{prev}");
                }
            }
            ui_cmd.env("LD_LIBRARY_PATH", &ld);
        }
        let st = ui_cmd.status().map_err(|e| e.to_string())?;
        if !st.success() {
            return Err("fenêtre de choix des modèles fermée sans validation".into());
        }
        let choice = offerings::read_setup_choice(home)
            .ok_or_else(|| "setup_choice.json manquant".to_string())?;
        offerings::apply_choice(home, &choice)?;
        return Ok(());
    }

    // Auto-best without UI (CI / headless).
    let choice = offerings::ModelSetupChoice {
        selected_ids: offer.recommended_ids.clone(),
        default_chat: offer
            .recommended_ids
            .iter()
            .find(|id| {
                offer
                    .models
                    .iter()
                    .any(|m| m.id == **id && m.profiles.iter().any(|p| p == "chat"))
            })
            .cloned()
            .unwrap_or_else(|| offer.recommended_ids.last().cloned().unwrap_or_default()),
        default_embed: offer
            .recommended_ids
            .iter()
            .find(|id| {
                offer
                    .models
                    .iter()
                    .any(|m| m.id == **id && m.profiles.iter().any(|p| p == "embed"))
            })
            .cloned()
            .unwrap_or_else(|| offer.recommended_ids.first().cloned().unwrap_or_default()),
        include_optional: false,
    };
    offerings::apply_choice(home, &choice)
}

fn ensure_installed_files_present(home: &Path) -> Result<(), String> {
    let inst = offerings::load_installed(home);
    if inst.models.is_empty() {
        // Fall back to legacy manifest download once.
        return bootstrap::ensure_models(home);
    }
    let dir = home.join("share/models");
    let mut missing = Vec::new();
    for m in &inst.models {
        let path = dir.join(&m.filename);
        if !path.exists() {
            missing.push(m.id.clone());
            continue;
        }
        if m.bytes > 0 {
            if let Ok(meta) = std::fs::metadata(&path) {
                let lo = m.bytes.saturating_mul(99) / 100;
                if meta.len() < lo {
                    missing.push(m.id.clone());
                }
            }
        }
    }
    if missing.is_empty() {
        return Ok(());
    }
    // Télécharge uniquement les manquants (nécessite catalog-offerings.json).
    offerings::download_ids(home, &missing)
}

/// Préfixe d'installation stable (indépendant du dossier d'extraction versionné).
fn default_install_home() -> PathBuf {
    if cfg!(windows) {
        let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into());
        PathBuf::from(base).join("AgentOS-Preview")
    } else {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(home).join(".local/share/agentos-preview")
    }
}

fn package_root_from_exe() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let bin_dir = exe.parent()?;
    if bin_dir.file_name().and_then(|s| s.to_str()) != Some("bin") {
        return None;
    }
    let home = bin_dir.parent()?;
    if home.join("VERSION").is_file() {
        Some(home.to_path_buf())
    } else {
        None
    }
}

fn same_dir(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

fn portable_requested(pkg: &Path) -> bool {
    if std::env::var("AOS_PORTABLE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        return true;
    }
    pkg.join(".portable").is_file()
}

/// True if this tree already holds user state we must not clobber.
fn user_data_present(home: &Path) -> bool {
    let markers = [
        "var/run/preferences.json",
        "var/run/onboarding.json",
        "var/models/installed.json",
        "var/secrets/vault.enc",
        "var/secrets/keys.yaml",
        "var/secrets/keys.yaml.migrated",
    ];
    if markers.iter().any(|m| home.join(m).is_file()) {
        return true;
    }
    for dir in [
        "var/sessions",
        "var/memory",
        "var/storage",
        "var/agents",
        "var/modules/notes",
        "var/feedback",
    ] {
        let p = home.join(dir);
        if let Ok(rd) = fs::read_dir(&p) {
            if rd.filter_map(|e| e.ok()).next().is_some() {
                return true;
            }
        }
    }
    false
}

fn migrate_user_data(from: &Path, to: &Path) -> Result<(), String> {
    if same_dir(from, to) || !user_data_present(from) || user_data_present(to) {
        return Ok(());
    }
    eprintln!(
        "[aos-session] migration des données utilisateur\n  de {}\n  vers {}",
        from.display(),
        to.display()
    );
    for name in ["var", "etc"] {
        let src = from.join(name);
        if !src.is_dir() {
            continue;
        }
        let dst = to.join(name);
        fs::create_dir_all(&dst).map_err(|e| e.to_string())?;
        bootstrap::copy_dir_recursive(&src, &dst).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Si on lance depuis un zip/dossier versionné, synchronise vers le préfixe
/// stable pour que mémoire / sessions / notes survivent aux nouvelles versions.
fn ensure_stable_install(pkg: &Path) -> Result<PathBuf, String> {
    let stable = default_install_home();
    if same_dir(pkg, &stable) {
        return Ok(stable);
    }
    if portable_requested(pkg) {
        eprintln!(
            "[aos-session] mode portable (AOS_PORTABLE ou .portable) — données dans {}",
            pkg.display()
        );
        return Ok(pkg.to_path_buf());
    }
    eprintln!(
        "[aos-session] installation stable : {} (préserve var/ etc/)",
        stable.display()
    );
    fs::create_dir_all(&stable).map_err(|e| e.to_string())?;
    migrate_user_data(pkg, &stable)?;
    update::overlay_program(&stable, pkg)?;
    // Seed empty dirs the install scripts create.
    for d in ["var", "etc", "var/mcp", "var/skills", "var/agents"] {
        let _ = fs::create_dir_all(stable.join(d));
    }
    Ok(stable)
}

/// Relance `bin/aos-session` (le nouveau binaire, pas `current_exe` qui peut
/// encore pointer vers `aos-session.exe.old` après le rename Windows).
fn reexec_updated(home: &Path) {
    let name = if cfg!(windows) {
        "aos-session.exe"
    } else {
        "aos-session"
    };
    let packaged = home.join("bin").join(name);
    let exe = if packaged.exists() {
        packaged
    } else {
        std::env::current_exe().unwrap_or_else(|_| PathBuf::from(name))
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut cmd = Command::new(&exe);
    cmd.args(&args).env("AOS_HOME", home).current_dir(home);
    match cmd.spawn() {
        Ok(_) => std::process::exit(0),
        Err(e) => eprintln!(
            "[aos-session] relance après update échouée : {e} — poursuite avec ce processus"
        ),
    }
}

fn resolve_home() -> PathBuf {
    if let Ok(h) = std::env::var("AOS_HOME") {
        return PathBuf::from(h);
    }
    if let Some(pkg) = package_root_from_exe() {
        match ensure_stable_install(&pkg) {
            Ok(home) => return home,
            Err(e) => {
                eprintln!(
                    "[aos-session] sync install stable échoué ({e}) — fallback {}",
                    pkg.display()
                );
                return pkg;
            }
        }
    }
    // Dev : racine du repo (cwd ou parent de target/).
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn bin_path(home: &Path, name: &str) -> PathBuf {
    let exe = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    let packaged = home.join("bin").join(&exe);
    if packaged.exists() {
        return packaged;
    }
    // Dev : target/release
    let dev = home.join("target").join("release").join(&exe);
    if dev.exists() {
        return dev;
    }
    // Fallback PATH / cwd
    PathBuf::from(exe)
}

fn ensure_layout(home: &Path) {
    for d in [
        "var/audit",
        "var/storage",
        "var/memory",
        "var/modules",
        "var/modules/src",
        "var/modules/build",
        "var/modules/packages",
        "var/skills",
        "var/secrets",
        "var/feedback",
        "var/run",
        "var/sessions",
        "var/updates",
        "var/updates/staging",
        "etc",
        "share/models",
        "share/models/lora",
        "share/models/vae",
        "share/models/styles",
        "share/models/upscale",
        "share/modules",
        "share/skills",
        "data/models",
        "skills",
    ] {
        let _ = fs::create_dir_all(home.join(d));
    }
    bootstrap::sweep_old_sidecars(&home.join("bin"));

    // Catalogue offerings (copie depuis le package / repo si absent).
    let offerings_dst = home.join("share/models/catalog-offerings.json");
    if !offerings_dst.exists() {
        let mut cands = vec![
            PathBuf::from("share/models/catalog-offerings.json"),
            home.join("share/models/catalog-offerings.json"),
        ];
        // Raccourci : à côté de bin/aos-session (package Preview).
        if let Ok(exe) = std::env::current_exe() {
            if let Some(bin) = exe.parent() {
                cands.push(bin.join("../share/models/catalog-offerings.json"));
                if let Some(home2) = bin.parent() {
                    cands.push(home2.join("share/models/catalog-offerings.json"));
                }
            }
        }
        for cand in cands {
            let Ok(cand) = cand.canonicalize() else {
                if cand.exists() && cand != offerings_dst {
                    let _ = fs::copy(&cand, &offerings_dst);
                    break;
                }
                continue;
            };
            if cand.exists() && cand != offerings_dst {
                let _ = fs::copy(&cand, &offerings_dst);
                break;
            }
        }
    }

    // Catalogue modèles (copie depuis le repo si absent).
    let catalog_dst = home.join("data/models/catalog.yaml");
    if !catalog_dst.exists() {
        for cand in [
            home.join("data/models/catalog.yaml"),
            PathBuf::from("data/models/catalog.yaml"),
        ] {
            if cand.exists() && cand != catalog_dst {
                let _ = fs::copy(&cand, &catalog_dst);
                break;
            }
        }
    }

    // Module notes : synchroniser le package partagé à chaque boot
    // (sinon un ancien WASM reste en place après update Preview — issue #1).
    let notes_share = home.join("share/modules/notes.aospkg");
    let notes_installed = home.join("var/modules/notes");
    if notes_share.exists() {
        if bootstrap::sync_packaged_module(&notes_share, &notes_installed) {
            let reg = home.join("var/modules/registry.yaml");
            if !reg.exists() {
                let _ = fs::write(
                    &reg,
                    r#"installed:
  - name: notes
    granted_caps:
      - fs.read:/documents/notes/**
      - fs.write:/documents/notes/**
      - mem.write:module:notes
      - mem.query:module:notes
    quarantined: false
"#,
                );
            }
        }
    }

    // Module tasks (Preview 0.3 / E3) — même resync au boot.
    let tasks_share = home.join("share/modules/tasks.aospkg");
    let tasks_installed = home.join("var/modules/tasks");
    if tasks_share.exists() {
        if bootstrap::sync_packaged_module(&tasks_share, &tasks_installed) {
            let reg = home.join("var/modules/registry.yaml");
            if let Ok(mut raw) = fs::read_to_string(&reg) {
                if !raw.contains("name: tasks") {
                    raw.push_str(
                        r#"
  - name: tasks
    granted_caps:
      - fs.read:/documents/tasks/**
      - fs.write:/documents/tasks/**
    quarantined: false
"#,
                    );
                    let _ = fs::write(&reg, raw);
                }
            } else {
                let _ = fs::write(
                    &reg,
                    r#"installed:
  - name: tasks
    granted_caps:
      - fs.read:/documents/tasks/**
      - fs.write:/documents/tasks/**
    quarantined: false
"#,
                );
            }
        }
    }

    // Module canvas (chat drawing) — même resync au boot.
    let canvas_share = home.join("share/modules/canvas.aospkg");
    let canvas_installed = home.join("var/modules/canvas");
    if canvas_share.exists() {
        if bootstrap::sync_packaged_module(&canvas_share, &canvas_installed) {
            let reg = home.join("var/modules/registry.yaml");
            if let Ok(mut raw) = fs::read_to_string(&reg) {
                if !raw.contains("name: canvas") {
                    // Match existing list indent (`- name:` vs `  - name:`).
                    let entry = if raw.lines().any(|l| l.starts_with("- name:")) {
                        r#"
- name: canvas
  granted_caps:
  - fs.write:/downloads/**
  quarantined: false
"#
                    } else {
                        r#"
  - name: canvas
    granted_caps:
      - fs.write:/downloads/**
    quarantined: false
"#
                    };
                    raw.push_str(entry);
                    let _ = fs::write(&reg, raw);
                }
            } else {
                let _ = fs::write(
                    &reg,
                    r#"installed:
  - name: canvas
    granted_caps:
      - fs.write:/downloads/**
    quarantined: false
"#,
                );
            }
        }
    }

    // Runtime scripté ext-rt (template pour modules agent) — package partagé.
    let extrt_share = home.join("share/modules/ext-rt.aospkg");
    let extrt_repo = home.join("modules/ext-rt.aospkg");
    if !extrt_share.join("module.wasm").exists() && extrt_repo.join("module.wasm").exists() {
        let _ = fs::create_dir_all(&extrt_share);
        bootstrap::copy_dir_recursive(&extrt_repo, &extrt_share).ok();
    }

    // Onboarding state
    let onboard = home.join("var/run/onboarding.json");
    if !onboard.exists() {
        let state = OnboardingState {
            completed: false,
            language: "fr".into(),
            routing: "local_only".into(),
            trust_default: "medium".into(),
            tutorial_step: 0,
        };
        let _ = fs::write(onboard, serde_json::to_string_pretty(&state).unwrap());
    }
}

fn write_runtime_configs(home: &Path) {
    let version = bootstrap::read_version(home);
    let hw = hardware::HardwareInfo::load(home).unwrap_or_else(|| hardware::probe(home));
    let vram_total = if hw.vram_mib > 0 { hw.vram_bytes() } else { 0 };
    let cpu_only = hw.tier == hardware::HardwareTier::Cpu || hw.vram_mib == 0;
    // Reserve embed + OS margin from placement budget (0 on CPU-only).
    let embed_reserve: u64 = if cpu_only { 0 } else { 1_073_741_824 };
    let os_reserve_vram = if cpu_only {
        0
    } else {
        embed_reserve + 1_073_741_824
    };

    let (default_chat, default_embed, entries) = offerings::runtime_model_entries(home);
    let mut models_yaml = String::new();
    let mut embed_path = home.join("share/models/missing-embed.gguf");
    for (id, o, path) in &entries {
        if id == &default_embed || o.profiles.iter().any(|p| p == "embed") {
            if id == &default_embed {
                embed_path = path.clone();
            }
        }
        models_yaml.push_str(&format!(
            r#"  {id}:
    path: {path}
    n_layers: {layers}
    weights_bytes: {weights}
    embed_bytes: {embed}
    kv_bytes_per_token: {kv}
    n_params: {params}
"#,
            id = id,
            path = path_yaml(path),
            layers = o.n_layers,
            weights = o.weights_bytes,
            embed = o.embed_bytes,
            kv = o.kv_bytes_per_token,
            params = o.n_params,
        ));
    }
    if models_yaml.is_empty() {
        // Absolute fallback legacy filenames.
        let instruct = resolve_model(
            home,
            "qwen2.5-3b-instruct-q4_k_m.gguf",
            &["share/models", "tools/models"],
        );
        let embed = resolve_model(
            home,
            "qwen2.5-0.5b-instruct-q4_k_m.gguf",
            &["share/models", "tools/models"],
        );
        embed_path = embed.clone();
        models_yaml = format!(
            r#"  local:embedded-instruct:
    path: {instruct}
    n_layers: 36
    weights_bytes: 2093886464
    embed_bytes: 175000000
    kv_bytes_per_token: 37000
    n_params: 3.4e9
  local:embedded-embed:
    path: {embed}
    n_layers: 24
    weights_bytes: 485436211
    embed_bytes: 76600000
    kv_bytes_per_token: 25000
    n_params: 6.3e8
"#,
            instruct = path_yaml(&instruct),
            embed = path_yaml(&embed),
        );
    }

    let modeld = home.join("etc/modeld.yaml");
    if !modeld.exists() || std::env::var_os("AOS_FORCE_CONFIG").is_some() {
        let yaml = format!(
            r#"# Généré par aos-session (Preview {version})
bus: "{BUS_ADDR}"
gpu: {gpu}
vram_total_bytes: {vram_total}
os_reserve_vram_bytes: {os_reserve_vram}
os_reserve_ram_bytes: 4294967296
default_model: {default_chat}
default_kv_tokens: 8192
n_threads: 8
n_seq_max: 4
batch_window_ms: 150
routing: local_only

models:
{models_yaml}"#,
            gpu = if cpu_only { "false" } else { "true" },
            vram_total = vram_total,
            os_reserve_vram = os_reserve_vram,
            default_chat = default_chat,
            models_yaml = models_yaml,
        );
        let _ = fs::write(&modeld, yaml);
    } else {
        // Migration douce : remonter le contexte KV pour les agents (évite PromptTooLong).
        if let Ok(raw) = fs::read_to_string(&modeld) {
            let mut next = raw.clone();
            if next.contains("default_kv_tokens: 2048") {
                next = next.replace("default_kv_tokens: 2048", "default_kv_tokens: 8192");
            }
            if next.contains("n_seq_max: 8\n") && next.contains("default_kv_tokens: 8192") {
                next = next.replace("n_seq_max: 8\n", "n_seq_max: 4\n");
            }
            if next != raw {
                let _ = fs::write(&modeld, next);
            }
        }
    }

    let platformd = home.join("etc/platformd.yaml");
    if !platformd.exists() || std::env::var_os("AOS_FORCE_CONFIG").is_some() {
        let yaml = format!(
            r#"# Généré par aos-session (Preview {version})
bus: "{BUS_ADDR}"
audit_dir: var/audit
storage_dir: var/storage
memory_dir: var/memory
modules_dir: var/modules
sessions_dir: var/sessions
secrets_file: var/secrets/keys.yaml
confirm_timeout_sec: 120
net_mode: offline_strict

embed_model:
  path: {embed}
  n_gpu_layers: 999
  n_threads: 8
"#,
            embed = path_yaml(&embed_path),
        );
        let _ = fs::write(&platformd, yaml);
    }

    // Refresh catalog.yaml entries for installed models (best-effort).
    let _ = default_embed;
    write_catalog_overlay(home, &entries);
}

fn write_catalog_overlay(home: &Path, entries: &[(String, offerings::ModelOffering, PathBuf)]) {
    let catalog_dst = home.join("data/models/catalog.yaml");
    let mut models = Vec::new();
    for (id, o, path) in entries {
        let is_embed = o.profiles.iter().any(|p| p == "embed") && o.profiles.len() == 1;
        let is_image =
            o.profiles.iter().any(|p| p == "image") || o.modality.as_deref() == Some("image");
        let is_tts =
            o.profiles.iter().any(|p| p == "tts") || o.modality.as_deref() == Some("audio");
        let (caps, modality, format, backends, offload) = if is_image {
            ("image", "image", o.format.as_str(), "sdcpp", "false")
        } else if is_tts {
            ("tts", "audio", o.format.as_str(), "piper", "false")
        } else if is_embed {
            ("embed", "embedding", "gguf", "llamacpp", "true")
        } else {
            ("chat, tools", "text", "gguf", "llamacpp", "true")
        };
        models.push(format!(
            r#"- id: {id}
  name: {name}
  modality: {modality}
  format: {format}
  source:
    type: local_file
    path: {path}
    sha256: "0000000000000000000000000000000000000000000000000000000000000000"
  architecture:
    n_layers: {layers}
    n_params: {params}
    context_length: 8192
  resource_hints:
    weights_bytes: {weights}
    embed_bytes: {embed}
    kv_bytes_per_token: {kv}
    supports_layer_offload: {offload}
  capabilities: [{caps}]
  backends_compatible: [{backends}]
  privacy_class: local
"#,
            id = id,
            name = o.name.replace(':', "-"),
            path = path_yaml(path),
            layers = o.n_layers,
            params = o.n_params,
            weights = o.weights_bytes,
            embed = o.embed_bytes,
            kv = o.kv_bytes_per_token,
        ));
    }
    if models.is_empty() {
        return;
    }
    let _ = fs::create_dir_all(home.join("data/models"));
    let body = format!(
        "# Généré par aos-session — modèles installés\nmodels:\n{}",
        models.join("\n")
    );
    let _ = fs::write(catalog_dst, body);
}

fn resolve_model(home: &Path, filename: &str, dirs: &[&str]) -> PathBuf {
    for d in dirs {
        let p = home.join(d).join(filename);
        if p.exists() {
            return p;
        }
    }
    // Chemin attendu même si absent (modeld échouera au load — message clair).
    home.join("share/models").join(filename)
}

fn path_yaml(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

fn inference_mode(home: &Path) -> String {
    std::fs::read_to_string(home.join("var/run/preferences.json"))
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .and_then(|v| {
            v.get("inference_mode")
                .and_then(|m| m.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "auto".into())
}

/// CUDA-linked `aos-modeld` vs `aos-modeld-cpu` (no CUDA DLL). Unified zip ships both.
/// On NVIDIA hosts the CUDA binary stays up for in-process pin cpu/gpu (E18);
/// `aos-modeld-cpu` is only for machines without NVIDIA.
fn pick_modeld_bin(home: &Path) -> (PathBuf, bool) {
    let nvidia = bootstrap::nvidia_ok();
    let mode = inference_mode(home);
    let cpu_bin = bin_path(home, "aos-modeld-cpu");
    let gpu_bin = bin_path(home, "aos-modeld");
    if nvidia && gpu_bin.exists() {
        return (gpu_bin, false);
    }
    let want_cpu =
        mode.eq_ignore_ascii_case("cpu") || (!nvidia && !mode.eq_ignore_ascii_case("gpu"));
    if want_cpu && cpu_bin.exists() {
        (cpu_bin, true)
    } else {
        (gpu_bin, false)
    }
}

fn modeld_command(home: &Path) -> Command {
    let (bin, cpu) = pick_modeld_bin(home);
    let mut cmd = Command::new(&bin);
    cmd.arg("etc/modeld.yaml");
    cmd.env("AOS_INFERENCE", inference_mode(home));
    if cpu {
        cmd.env("AOS_CPU_ONLY", "1");
    } else {
        cmd.env_remove("AOS_CPU_ONLY");
    }
    eprintln!(
        "[aos-session] modeld {} (cpu={cpu})",
        bin.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("aos-modeld")
    );
    cmd
}

fn daemon_env(cmd: &mut Command, home: &Path) {
    cmd.current_dir(home).env("AOS_HOME", home);
    #[cfg(target_os = "linux")]
    {
        let bin_dir = home.join("bin");
        let mut ld = bin_dir.to_string_lossy().to_string();
        if let Ok(prev) = std::env::var("LD_LIBRARY_PATH") {
            if !prev.is_empty() {
                ld = format!("{ld}:{prev}");
            }
        }
        cmd.env("LD_LIBRARY_PATH", ld);
    }
}

fn start_daemons(session: &Arc<Session>) -> Result<(), String> {
    let home = &session.home;
    let bin = |n: &str| bin_path(home, n);

    let spawn = |name: &'static str, mut cmd: Command| -> Result<Daemon, String> {
        let log_path = home.join("var/run").join(format!("{name}.stderr.log"));
        let log_file = fs::File::create(&log_path)
            .map_err(|e| format!("{name}: log {e} ({})", log_path.display()))?;
        daemon_env(&mut cmd, home);
        cmd.stdout(Stdio::null())
            // Piped unread stderr deadlocks GPU daemons (ggml/CUDA logs).
            .stderr(Stdio::from(log_file));
        let child = cmd
            .spawn()
            .map_err(|e| format!("{name}: {e} ({})", bin(name).display()))?;
        let pid = child.id();
        let _ = fs::write(
            home.join("var/run").join(format!("{name}.pid")),
            pid.to_string(),
        );
        eprintln!("[aos-session] {name} up (pid {pid})");
        Ok(Daemon { name, child })
    };

    let mut list = Vec::new();

    {
        let mut cmd = Command::new(bin("aos-busd"));
        cmd.arg("24701");
        list.push(spawn("aos-busd", cmd)?);
    }
    thread::sleep(Duration::from_millis(800));

    {
        let mut cmd = Command::new(bin("aos-capkd"));
        cmd.arg(BUS_ADDR);
        list.push(spawn("aos-capkd", cmd)?);
    }
    {
        let mut cmd = Command::new(bin("aos-auditd"));
        cmd.arg(BUS_ADDR).arg("var/audit");
        list.push(spawn("aos-auditd", cmd)?);
    }
    {
        let cmd = modeld_command(home);
        list.push(spawn("aos-modeld", cmd)?);
    }
    {
        let mut cmd = Command::new(bin("aos-platformd"));
        cmd.arg("etc/platformd.yaml");
        list.push(spawn("aos-platformd", cmd)?);
    }
    {
        let mut cmd = Command::new(bin("aos-agentd"));
        cmd.arg(BUS_ADDR);
        list.push(spawn("aos-agentd", cmd)?);
    }

    thread::sleep(Duration::from_secs(2));
    *session.daemons.lock().unwrap() = list;
    Ok(())
}

fn healthcheck() -> Result<(), String> {
    let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    rt.block_on(async {
        let mut last = String::new();
        for _ in 0..30 {
            match BusClient::connect(BUS_ADDR, "session-health").await {
                Ok(bus) => {
                    let probes = [
                        ("modeld", "model.list"),
                        ("agentd", "agent.list"),
                        ("platformd", "module.list"),
                        ("capkd", "cap.check"),
                    ];
                    let mut ok = true;
                    for (name, intent) in probes {
                        if !bus.lookup(intent).await.unwrap_or(false) {
                            ok = false;
                            last = format!("{name} ({intent}) absent");
                            break;
                        }
                    }
                    if ok {
                        return Ok(());
                    }
                }
                Err(e) => last = e.to_string(),
            }
            thread::sleep(Duration::from_millis(500));
        }
        Err(last)
    })
}

/// Applique `trust_default` de l'onboarding au Trust Manager (`__default__`).
fn apply_trust_default(home: &Path) {
    let onboard = home.join("var/run/onboarding.json");
    let Ok(raw) = fs::read_to_string(&onboard) else {
        return;
    };
    let Ok(state) = serde_json::from_str::<OnboardingState>(&raw) else {
        return;
    };
    let score = match state.trust_default.as_str() {
        "low" | "basse" => 0.2,
        "high" | "haute" => 0.85,
        _ => 0.5, // medium
    };
    let rt = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(_) => return,
    };
    rt.block_on(async {
        let Ok(bus) = BusClient::connect(BUS_ADDR, "session-trust").await else {
            return;
        };
        let _ = bus
            .call::<aos_proto::TrustSetRequest, bool>(
                "trust.set",
                &aos_proto::TrustSetRequest {
                    agent_id: "__default__".into(),
                    score,
                },
                vec![],
            )
            .await;
        eprintln!(
            "[aos-session] trust_default={} → score {score}",
            state.trust_default
        );
    });
}

fn stop_all(session: &Arc<Session>) {
    let mut daemons = session.daemons.lock().unwrap();
    // Arrêt inverse ; tuer aussi les workers agents.
    kill_by_name("aos-agent-worker");
    for d in daemons.iter_mut().rev() {
        let _ = d.child.kill();
        let _ = d.child.wait();
        eprintln!("[aos-session] {} stopped", d.name);
    }
    daemons.clear();
}

fn kill_by_name(name: &str) {
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/F", "/IM", &format!("{name}.exe")])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(not(windows))]
    {
        let _ = Command::new("pkill")
            .args(["-x", name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

fn auditd_watchdog(session: Arc<Session>) {
    daemon_watchdog(session, "aos-auditd", &|home| {
        let mut cmd = Command::new(bin_path(home, "aos-auditd"));
        cmd.arg(BUS_ADDR).arg("var/audit");
        cmd
    });
}

/// Redémarre platformd s'il meurt (ex. assert llama embed) pour que
/// `mem.*` / notes / modules restent joignables.
fn platformd_watchdog(session: Arc<Session>) {
    daemon_watchdog(session, "aos-platformd", &|home| {
        let mut cmd = Command::new(bin_path(home, "aos-platformd"));
        cmd.arg("etc/platformd.yaml");
        cmd
    });
}

fn modeld_watchdog(session: Arc<Session>) {
    daemon_watchdog(session, "aos-modeld", &|home| modeld_command(home));
}

fn daemon_watchdog(session: Arc<Session>, name: &'static str, make_cmd: &dyn Fn(&Path) -> Command) {
    while !session.stop.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_secs(2));
        if session.stop.load(Ordering::SeqCst) {
            break;
        }
        let mut daemons = session.daemons.lock().unwrap();
        let Some(pos) = daemons.iter().position(|d| d.name == name) else {
            continue;
        };
        // try_wait : None = encore vivant
        match daemons[pos].child.try_wait() {
            Ok(Some(_)) => {
                eprintln!("[aos-session] {name} mort — redémarrage");
                let home = session.home.clone();
                let mut cmd = make_cmd(&home);
                daemon_env(&mut cmd, &home);
                // Conserver les logs stderr (crash GGML, etc.)
                let log_path = home.join("var/run").join(format!("{name}.stderr.log"));
                let stderr = fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&log_path)
                    .ok()
                    .map(Stdio::from)
                    .unwrap_or_else(Stdio::null);
                cmd.stdout(Stdio::null()).stderr(stderr);
                match cmd.spawn() {
                    Ok(child) => {
                        let pid = child.id();
                        let _ = fs::write(
                            home.join("var/run").join(format!("{name}.pid")),
                            pid.to_string(),
                        );
                        daemons[pos] = Daemon { name, child };
                        eprintln!("[aos-session] {name} up (pid {pid})");
                    }
                    Err(e) => eprintln!("[aos-session] restart {name} échoué : {e}"),
                }
            }
            Ok(None) => {}
            Err(_) => {}
        }
    }
}

fn ctrlc_guard(session: Arc<Session>) {
    let _ = ctrlc::set_handler(move || {
        eprintln!("[aos-session] signal — arrêt");
        session.stop.store(true, Ordering::SeqCst);
        stop_all(&session);
        std::process::exit(130);
    });
}
