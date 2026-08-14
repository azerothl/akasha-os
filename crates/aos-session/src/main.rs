//! `aos-session` — superviseur Preview.
//!
//! Remplace `run-demo.ps1` pour les testeurs : résout `AOS_HOME`, génère les
//! configs relatives, démarre les daemons, lance egui, arrête proprement.

mod bootstrap;
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
            Some(info) => match update::download_update(&home, &info) {
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
            },
            None => {
                eprintln!("[aos-session] aucune offre de mise à jour (var/run/update_available.json)");
                std::process::exit(1);
            }
        }
    }

    let home = resolve_home();
    std::env::set_var("AOS_HOME", &home);
    std::env::set_current_dir(&home).expect("chdir AOS_HOME");

    let version = bootstrap::read_version(&home);
    eprintln!("[aos-session] Agent OS Preview {version}");
    eprintln!("[aos-session] AOS_HOME={}", home.display());

    // Appliquer une update téléchargée avant de toucher aux binaires en cours.
    match update::apply_pending_update(&home) {
        Ok(true) => eprintln!("[aos-session] redémarrage recommandé après update"),
        Ok(false) => {}
        Err(e) => eprintln!("[aos-session] apply update : {e}"),
    }

    ensure_layout(&home);
    bootstrap::ensure_skills(&home);

    if let Err(e) = bootstrap::check_disk_space(&home) {
        bootstrap::show_fatal_dialog(
            "Agent OS Preview — disque",
            &format!("{e}\n\nLibérez de l'espace puis relancez. Voir FIRST-RUN.md."),
        );
        std::process::exit(3);
    }

    if !bootstrap::nvidia_ok() {
        bootstrap::show_fatal_dialog(
            "Agent OS Preview — GPU NVIDIA requis",
            "nvidia-smi introuvable ou en échec.\n\
             Preview 0.1 n'accepte pas le fallback CPU.\n\
             Installez un driver NVIDIA récent, puis relancez.\n\
             Voir INSTALL.md / FIRST-RUN.md.",
        );
        std::process::exit(2);
    }

    if let Err(e) = bootstrap::ensure_models(&home) {
        bootstrap::show_fatal_dialog(
            "Agent OS Preview — modèles",
            &format!(
                "Échec du téléchargement des modèles GGUF :\n{e}\n\n\
                 Vérifiez le réseau, ou placez les fichiers dans share/models/.\n\
                 Voir share/models/manifest.json et FIRST-RUN.md."
            ),
        );
        std::process::exit(4);
    }

    write_runtime_configs(&home);

    let session = Arc::new(Session {
        home: home.clone(),
        daemons: Mutex::new(Vec::new()),
        stop: AtomicBool::new(false),
    });

    {
        let s = session.clone();
        ctrlc_guard(s);
    }

    if let Err(e) = start_daemons(&session) {
        eprintln!("[aos-session] démarrage échoué : {e}");
        eprintln!(
            "[aos-session] Astuce : consultez var/run/*.stderr.log (GPU, modèles, bus)."
        );
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
    apply_trust_default(&home);

    // Check updates en arrière-plan (best-effort, repo public).
    {
        let home_bg = home.clone();
        let ver_bg = version.clone();
        thread::spawn(move || {
            match update::check_latest(&ver_bg) {
                Ok(Some(info)) => {
                    let _ = update::write_update_offer(&home_bg, &info);
                    eprintln!(
                        "[aos-session] mise à jour disponible : {} ({})",
                        info.version, info.html_url
                    );
                }
                Ok(None) => update::clear_update_offer(&home_bg),
                Err(e) => eprintln!("[aos-session] check update : {e}"),
            }
        });
    }

    {
        let s = session.clone();
        thread::spawn(move || auditd_watchdog(s));
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

    session.stop.store(true, Ordering::SeqCst);
    stop_all(&session);

    match status {
        Ok(st) if st.success() => {}
        Ok(st) => {
            eprintln!("[aos-session] UI exit {st}");
            std::process::exit(st.code().unwrap_or(1));
        }
        Err(e) => {
            eprintln!("[aos-session] impossible de lancer {} : {e}", ui.display());
            std::process::exit(1);
        }
    }
}

fn resolve_home() -> PathBuf {
    if let Ok(h) = std::env::var("AOS_HOME") {
        return PathBuf::from(h);
    }
    // Exécutable dans <home>/bin/aos-session → home = parent du bin.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(bin_dir) = exe.parent() {
            if bin_dir.file_name().and_then(|s| s.to_str()) == Some("bin") {
                if let Some(home) = bin_dir.parent() {
                    return home.to_path_buf();
                }
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
        "share/modules",
        "share/skills",
        "data/models",
        "skills",
    ] {
        let _ = fs::create_dir_all(home.join(d));
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

    // Module notes : partager → installer au premier boot via copie package.
    let notes_share = home.join("share/modules/notes.aospkg");
    let notes_installed = home.join("var/modules/notes");
    if notes_share.exists() && !notes_installed.join("module.wasm").exists() {
        let _ = fs::remove_dir_all(&notes_installed);
        bootstrap::copy_dir_recursive(&notes_share, &notes_installed).ok();
        let reg = home.join("var/modules/registry.yaml");
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
    let model_instruct = resolve_model(
        home,
        "qwen2.5-3b-instruct-q4_k_m.gguf",
        &["share/models", "tools/models"],
    );
    let model_embed = resolve_model(
        home,
        "qwen2.5-0.5b-instruct-q4_k_m.gguf",
        &["share/models", "tools/models"],
    );

    let modeld = home.join("etc/modeld.yaml");
    if !modeld.exists() || std::env::var_os("AOS_FORCE_CONFIG").is_some() {
        let yaml = format!(
            r#"# Généré par aos-session (Preview {version})
bus: "{BUS_ADDR}"
gpu: true
vram_total_bytes: 12884901888
os_reserve_vram_bytes: 1073741824
os_reserve_ram_bytes: 4294967296
default_model: local:embedded-instruct
default_kv_tokens: 2048
n_threads: 8
n_seq_max: 8
batch_window_ms: 150
routing: local_only

models:
  local:embedded-instruct:
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
            instruct = path_yaml(&model_instruct),
            embed = path_yaml(&model_embed),
        );
        let _ = fs::write(&modeld, yaml);
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
            embed = path_yaml(&model_embed),
        );
        let _ = fs::write(&platformd, yaml);
    }
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
        let _ = fs::write(home.join("var/run").join(format!("{name}.pid")), pid.to_string());
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
        let mut cmd = Command::new(bin("aos-modeld"));
        cmd.arg("etc/modeld.yaml");
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
    while !session.stop.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_secs(2));
        if session.stop.load(Ordering::SeqCst) {
            break;
        }
        let mut daemons = session.daemons.lock().unwrap();
        let Some(pos) = daemons.iter().position(|d| d.name == "aos-auditd") else {
            continue;
        };
        // try_wait : None = encore vivant
        match daemons[pos].child.try_wait() {
            Ok(Some(_)) => {
                eprintln!("[aos-session] auditd mort — redémarrage");
                let home = session.home.clone();
                let mut cmd = Command::new(bin_path(&home, "aos-auditd"));
                cmd.arg(BUS_ADDR).arg("var/audit");
                daemon_env(&mut cmd, &home);
                cmd.stdout(Stdio::null()).stderr(Stdio::null());
                match cmd.spawn() {
                    Ok(child) => {
                        let pid = child.id();
                        let _ = fs::write(home.join("var/run/aos-auditd.pid"), pid.to_string());
                        daemons[pos] = Daemon {
                            name: "aos-auditd",
                            child,
                        };
                        eprintln!("[aos-session] aos-auditd up (pid {pid})");
                    }
                    Err(e) => eprintln!("[aos-session] restart auditd échoué : {e}"),
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
