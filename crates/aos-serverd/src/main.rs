//! `aos-serverd` binary — P21.3 headless owner + local control plane.
//!
//! `serve` / `--headless` boots busd…agentd (no egui), healthchecks, runs
//! soft + hard watchdogs, and serves `status` / `restart` / `stop` on the
//! local control socket. See ADR 0012.

use aos_serverd::{
    agentd_command, auditd_command, control_endpoint_present, control_socket_path, healthcheck,
    modeld_command, platformd_command, scaffold_status, send_control, serve_control,
    serverd_pid_relpath, serverd_spawn_opts, ControlRequest, ControlState, ProcessTree,
    SessionHandoff, SpawnOptions, DAEMON_BOOT_ORDER, WATCHDOG_HARD, WATCHDOG_SOFT,
};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

fn main() {
    let mut args = env::args().skip(1);
    let mut home: Option<PathBuf> = env::var_os("AOS_HOME").map(PathBuf::from);
    let mut cmd = String::from("status");

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                print_help();
                return;
            }
            "--aos-home" => {
                home = args.next().map(PathBuf::from);
            }
            "--headless" | "serve" | "run" => cmd = "serve".into(),
            "status" | "handoff" | "restart" | "stop" => cmd = arg,
            other if other.starts_with("--aos-home=") => {
                home = Some(PathBuf::from(other.trim_start_matches("--aos-home=")));
            }
            other => {
                eprintln!("aos-serverd: unknown argument `{other}` (try --help)");
                std::process::exit(2);
            }
        }
    }

    let home = home.unwrap_or_else(|| PathBuf::from("."));
    match cmd.as_str() {
        "handoff" => print_handoff(),
        "serve" => {
            if let Err(e) = run_headless(&home) {
                eprintln!("[aos-serverd] {e}");
                std::process::exit(1);
            }
        }
        "restart" | "stop" => {
            if let Err(e) = send_live_command(&home, &cmd) {
                eprintln!("[aos-serverd] {e}");
                std::process::exit(1);
            }
        }
        _ => {
            if let Err(e) = print_status(&home) {
                eprintln!("[aos-serverd] {e}");
                std::process::exit(1);
            }
        }
    }
}

fn print_help() {
    println!(
        "aos-serverd — Akasha OS Preview server lifecycle daemon (P21.3)\n\n\
Usage:\n  aos-serverd [--aos-home <path>] [status|handoff|serve|restart|stop]\n  aos-serverd --headless\n\n\
serve / --headless: own the Preview process tree without egui (Ctrl+C stops).\n\
status / restart / stop: query the local control socket when serve is up.\n\
Requires AOS_HOME already laid out (etc/*.yaml, bin/*). See ADR 0012."
    );
}

fn print_status(home: &Path) -> Result<(), String> {
    if control_endpoint_present(home) {
        let resp = send_control(
            home,
            &ControlRequest {
                cmd: "status".into(),
                actor: "cli".into(),
            },
        )?;
        println!("{}", resp.to_line());
        if !resp.ok {
            return Err(resp.message.unwrap_or_else(|| "status failed".into()));
        }
        return Ok(());
    }

    let st = scaffold_status();
    println!("aos-serverd ({}) — no live control endpoint", st.lot);
    println!("  lib_spawns_daemons: {}", st.lib_spawns_daemons);
    println!("  binary_owns_tree:   {}", st.binary_owns_tree);
    println!("  control_plane_live: {}", st.control_plane_live);
    println!("  aos_home:           {}", home.display());
    println!(
        "  control_path:       {}",
        control_socket_path(home).display()
    );
    println!("  boot_order:         {}", DAEMON_BOOT_ORDER.join(" → "));
    println!("  soft watchdogs:     {}", WATCHDOG_SOFT.join(", "));
    println!(
        "  hard watchdogs:     {} (ordered tree restart + backoff)",
        WATCHDOG_HARD.join(", ")
    );
    println!();
    println!("Run: aos-serverd serve   (or --headless)");
    Ok(())
}

fn send_live_command(home: &Path, cmd: &str) -> Result<(), String> {
    if !control_endpoint_present(home) {
        return Err(format!(
            "no live control endpoint at {} — is serve running?",
            control_socket_path(home).display()
        ));
    }
    let resp = send_control(
        home,
        &ControlRequest {
            cmd: cmd.into(),
            actor: "cli".into(),
        },
    )?;
    println!("{}", resp.to_line());
    if resp.ok {
        Ok(())
    } else {
        Err(resp.message.unwrap_or_else(|| format!("{cmd} failed")))
    }
}

fn print_handoff() {
    println!("Session handoff modes (ADR 0012):");
    for mode in [
        SessionHandoff::LegacySpawn,
        SessionHandoff::BootstrapThenAttach,
        SessionHandoff::AttachUiOnly,
    ] {
        let (label, desc) = match mode {
            SessionHandoff::LegacySpawn => (
                "legacy-spawn",
                "aos-session spawns via lifecycle lib (0.18 UX; avoid if serverd serve is up)",
            ),
            SessionHandoff::BootstrapThenAttach => (
                "bootstrap-then-attach",
                "session ensures serverd is up, then attaches UI (P21.6)",
            ),
            SessionHandoff::AttachUiOnly => (
                "attach-ui-only",
                "serverd owns the tree; session only launches egui (P21.6)",
            ),
        };
        println!("  {label:24} {desc}");
    }
}

fn run_headless(home: &Path) -> Result<(), String> {
    let run_dir = home.join("var/run");
    fs::create_dir_all(&run_dir).map_err(|e| format!("var/run: {e}"))?;

    let opts = serverd_spawn_opts();
    eprintln!(
        "[aos-serverd] headless start (gpu_accel={}, home={})",
        opts.gpu_accel,
        home.display()
    );

    let tree = Arc::new(Mutex::new(ProcessTree::empty(home, "aos-serverd")));
    {
        let mut t = tree.lock().unwrap();
        t.start(&opts)?;
    }
    if let Err(e) = healthcheck() {
        tree.lock().unwrap().stop();
        return Err(format!("healthcheck échoué : {e}"));
    }
    eprintln!("[aos-serverd] services OK (no egui)");

    let _ = fs::write(
        home.join(serverd_pid_relpath()),
        std::process::id().to_string(),
    );

    let stop = Arc::new(AtomicBool::new(false));
    let state = Arc::new(ControlState {
        home: home.to_path_buf(),
        tree: tree.clone(),
        opts: opts.clone(),
        stop: stop.clone(),
        tree_restart_backoff_secs: Mutex::new(0),
    });

    {
        let stop_c = stop.clone();
        let tree_c = tree.clone();
        let _ = ctrlc::set_handler(move || {
            eprintln!("[aos-serverd] signal — arrêt");
            stop_c.store(true, Ordering::SeqCst);
            tree_c.lock().unwrap().stop();
            std::process::exit(130);
        });
    }

    {
        let state_c = state.clone();
        thread::spawn(move || serve_control(state_c));
    }

    spawn_soft_watchdogs(tree.clone(), stop.clone(), opts.clone());
    spawn_hard_watchdogs(state.clone());

    while !stop.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_secs(1));
    }
    tree.lock().unwrap().stop();
    Ok(())
}

fn spawn_soft_watchdogs(tree: Arc<Mutex<ProcessTree>>, stop: Arc<AtomicBool>, opts: SpawnOptions) {
    type Maker = Box<dyn Fn(&Path) -> std::process::Command + Send>;
    let modeld_opts = opts.clone();
    let watchers: Vec<(&'static str, Maker)> = vec![
        ("aos-auditd", Box::new(auditd_command)),
        ("aos-platformd", Box::new(platformd_command)),
        (
            "aos-modeld",
            Box::new(move |h| modeld_command(h, &modeld_opts)),
        ),
        ("aos-agentd", Box::new(agentd_command)),
    ];

    for (name, make_cmd) in watchers {
        let tree = tree.clone();
        let stop = stop.clone();
        thread::spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_secs(2));
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                let mut t = tree.lock().unwrap();
                let Some(pos) = t.daemons().iter().position(|d| d.name == name) else {
                    continue;
                };
                if let Ok(Some(_)) = t.daemons_mut()[pos].child.try_wait() {
                    eprintln!("[aos-serverd] {name} mort — soft respawn");
                    let _ = t.respawn(name, &*make_cmd);
                }
            }
        });
    }
}

/// busd / capkd death → ordered tree restart with backoff (P21.3).
fn spawn_hard_watchdogs(state: Arc<ControlState>) {
    for name in WATCHDOG_HARD {
        let state = state.clone();
        let name = *name;
        thread::spawn(move || {
            while !state.stop.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_secs(2));
                if state.stop.load(Ordering::SeqCst) {
                    break;
                }
                let dead = {
                    let mut t = state.tree.lock().unwrap();
                    let Some(pos) = t.daemons().iter().position(|d| d.name == name) else {
                        continue;
                    };
                    matches!(t.daemons_mut()[pos].child.try_wait(), Ok(Some(_)))
                };
                if dead {
                    eprintln!("[aos-serverd] {name} mort — ordered tree restart");
                    if let Err(e) = state.ordered_restart() {
                        eprintln!("[aos-serverd] ordered restart failed: {e}");
                    }
                }
            }
        });
    }
}
