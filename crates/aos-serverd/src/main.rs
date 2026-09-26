//! `aos-serverd` binary — P21.6 headless owner + control + intake + optional mcpd/bridged.
//!
//! `serve` / `--headless` boots busd…agentd (no egui), optional mcpd/bridged when
//! `etc/serverd.yaml` opts in, healthchecks, soft + hard watchdogs, serves
//! `status` / `restart` / `stop`, and accepts agent intake on the local control
//! socket. See ADR 0012.

use aos_serverd::{
    agentd_command, auditd_command, bridged_command, control_endpoint_present, control_socket_path,
    healthcheck, list_jobs, mcpd_command, modeld_command, platformd_command, scaffold_status,
    send_control, serve_control, serverd_pid_relpath, serverd_spawn_opts_for, ControlRequest,
    ControlState, ProcessTree, SessionHandoff, SpawnOptions, DAEMON_BOOT_ORDER, WATCHDOG_HARD,
    WATCHDOG_SOFT,
};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut home: Option<PathBuf> = env::var_os("AOS_HOME").map(PathBuf::from);
    let mut positional: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => {
                print_help();
                return;
            }
            "--aos-home" => {
                i += 1;
                home = args.get(i).map(PathBuf::from);
            }
            other if other.starts_with("--aos-home=") => {
                home = Some(PathBuf::from(other.trim_start_matches("--aos-home=")));
            }
            "--headless" => positional.push("serve".into()),
            other => positional.push(other.to_string()),
        }
        i += 1;
    }

    let home = home.unwrap_or_else(|| PathBuf::from("."));
    let cmd = positional.first().map(|s| s.as_str()).unwrap_or("status");

    match cmd {
        "handoff" => print_handoff(),
        "serve" | "run" => {
            if let Err(e) = run_headless(&home) {
                eprintln!("[aos-serverd] {e}");
                std::process::exit(1);
            }
        }
        "restart" | "stop" => {
            if let Err(e) = send_live_command(
                &home,
                &ControlRequest {
                    cmd: cmd.into(),
                    actor: "cli".into(),
                    goal: None,
                    agent_id: None,
                    model_id: None,
                    caps: Vec::new(),
                    mode: None,
                    job_id: None,
                    interval_secs: None,
                },
            ) {
                eprintln!("[aos-serverd] {e}");
                std::process::exit(1);
            }
        }
        "enqueue" | "enqueue-agent" => {
            if let Err(e) = run_enqueue(&home, &positional[1..]) {
                eprintln!("[aos-serverd] {e}");
                std::process::exit(1);
            }
        }
        "jobs" | "job-list" => {
            if let Err(e) = run_job_list(&home) {
                eprintln!("[aos-serverd] {e}");
                std::process::exit(1);
            }
        }
        "job" | "job-status" => {
            let id = positional.get(1).map(|s| s.as_str()).unwrap_or("");
            if let Err(e) = run_job_status(&home, id) {
                eprintln!("[aos-serverd] {e}");
                std::process::exit(1);
            }
        }
        "status" => {
            if let Err(e) = print_status(&home) {
                eprintln!("[aos-serverd] {e}");
                std::process::exit(1);
            }
        }
        other => {
            eprintln!("aos-serverd: unknown command `{other}` (try --help)");
            std::process::exit(2);
        }
    }
}

fn print_help() {
    println!(
        "aos-serverd — Akasha OS Preview server lifecycle daemon (P21.6)\n\n\
Usage:\n  aos-serverd [--aos-home <path>] status|handoff|serve|restart|stop\n  aos-serverd --headless\n  aos-serverd enqueue --goal <text> [--actor <id>] [--mode create|start|schedule]\n                      [--agent-id <id>] [--model-id <id>] [--interval <secs>]\n  aos-serverd jobs\n  aos-serverd job <job-id>\n\n\
serve / --headless: own the Preview process tree without egui (Ctrl+C stops).\n\
enqueue: intake façade → agent.create / agent.start / schedule.create (caps fail-closed).\n\
Opt-in (etc/serverd.yaml): supervise_bridged / supervise_mcpd (default off).\n\
Session attach: aos-session uses handoff modes (see `handoff`); closing egui does not\n\
stop the tree when serverd owns it.\n\n\
Local control only (Unix socket / Windows pipe under $AOS_HOME/var/run/).\n\
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
                goal: None,
                agent_id: None,
                model_id: None,
                caps: Vec::new(),
                mode: None,
                job_id: None,
                interval_secs: None,
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
    println!("  agent_intake_live:  {}", st.agent_intake_live);
    println!(
        "  optional+attach:   {}",
        st.optional_daemons_and_attach
    );
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
    println!("  recorded jobs:      {}", list_jobs(home).len());
    println!();
    println!("Run: aos-serverd serve   (or --headless)");
    Ok(())
}

fn send_live_command(home: &Path, req: &ControlRequest) -> Result<(), String> {
    if !control_endpoint_present(home) {
        return Err(format!(
            "no live control endpoint at {} — is serve running?",
            control_socket_path(home).display()
        ));
    }
    let resp = send_control(home, req)?;
    println!("{}", resp.to_line());
    if resp.ok {
        Ok(())
    } else {
        Err(resp
            .message
            .unwrap_or_else(|| format!("{} failed", req.cmd)))
    }
}

fn run_enqueue(home: &Path, args: &[String]) -> Result<(), String> {
    let mut goal: Option<String> = None;
    let mut actor = String::from("cli");
    let mut mode: Option<String> = None;
    let mut agent_id: Option<String> = None;
    let mut model_id: Option<String> = None;
    let mut interval_secs: Option<u64> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--goal" => {
                i += 1;
                goal = args.get(i).cloned();
            }
            "--actor" => {
                i += 1;
                actor = args.get(i).cloned().unwrap_or_default();
            }
            "--mode" => {
                i += 1;
                mode = args.get(i).cloned();
            }
            "--agent-id" => {
                i += 1;
                agent_id = args.get(i).cloned();
            }
            "--model-id" => {
                i += 1;
                model_id = args.get(i).cloned();
            }
            "--interval" => {
                i += 1;
                interval_secs = args
                    .get(i)
                    .and_then(|s| s.parse::<u64>().ok());
            }
            other if other.starts_with("--goal=") => {
                goal = Some(other.trim_start_matches("--goal=").into());
            }
            other => {
                return Err(format!("unknown enqueue flag `{other}`"));
            }
        }
        i += 1;
    }
    send_live_command(
        home,
        &ControlRequest {
            cmd: "enqueue-agent".into(),
            actor,
            goal,
            agent_id,
            model_id,
            caps: Vec::new(),
            mode,
            job_id: None,
            interval_secs,
        },
    )
}

fn run_job_list(home: &Path) -> Result<(), String> {
    if control_endpoint_present(home) {
        return send_live_command(
            home,
            &ControlRequest {
                cmd: "job-list".into(),
                actor: "cli".into(),
                goal: None,
                agent_id: None,
                model_id: None,
                caps: Vec::new(),
                mode: None,
                job_id: None,
                interval_secs: None,
            },
        );
    }
    let jobs = list_jobs(home);
    println!("{}", serde_json::to_string_pretty(&jobs).unwrap_or_default());
    Ok(())
}

fn run_job_status(home: &Path, id: &str) -> Result<(), String> {
    if id.is_empty() {
        return Err("usage: aos-serverd job <job-id>".into());
    }
    if control_endpoint_present(home) {
        return send_live_command(
            home,
            &ControlRequest {
                cmd: "job-status".into(),
                actor: "cli".into(),
                goal: None,
                agent_id: None,
                model_id: None,
                caps: Vec::new(),
                mode: None,
                job_id: Some(id.into()),
                interval_secs: None,
            },
        );
    }
    match aos_serverd::get_job(home, id) {
        Some(job) => {
            println!("{}", serde_json::to_string_pretty(&job).unwrap_or_default());
            Ok(())
        }
        None => Err(format!("job not found: {id}")),
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

    let opts = serverd_spawn_opts_for(home);
    eprintln!(
        "[aos-serverd] headless start (gpu_accel={}, bridged={}, mcpd={}, home={})",
        opts.gpu_accel,
        opts.supervise_bridged,
        opts.supervise_mcpd,
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
    let mut watchers: Vec<(&'static str, Maker)> = vec![
        ("aos-auditd", Box::new(auditd_command)),
        ("aos-platformd", Box::new(platformd_command)),
        (
            "aos-modeld",
            Box::new(move |h| modeld_command(h, &modeld_opts)),
        ),
        ("aos-agentd", Box::new(agentd_command)),
    ];
    if opts.supervise_bridged {
        watchers.push(("aos-bridged", Box::new(bridged_command)));
    }
    if opts.supervise_mcpd {
        watchers.push(("aos-mcpd", Box::new(mcpd_command)));
    }

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
