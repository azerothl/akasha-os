//! `aos-serverd` binary — P21.0 scaffold only.
//!
//! Does **not** spawn the Preview process tree yet (that is P21.2). Prints
//! contract status and exits. See ADR 0012 and `aos_serverd` lib docs.

use aos_serverd::{
    control_socket_path, scaffold_status, SessionHandoff, DAEMON_BOOT_ORDER, WATCHDOG_BASELINE,
    WATCHDOG_P21_EXTRA,
};
use std::env;
use std::path::PathBuf;

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
            "status" | "handoff" => cmd = arg,
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
        _ => print_status(&home),
    }
}

fn print_help() {
    println!(
        "aos-serverd — Akasha OS Preview server lifecycle daemon (P21.0 scaffold)\n\n\
Usage:\n  aos-serverd [--aos-home <path>] [status|handoff]\n\n\
P21.0 does not spawn daemons yet. See ADR 0012."
    );
}

fn print_status(home: &std::path::Path) {
    let st = scaffold_status();
    println!("aos-serverd scaffold ({})", st.lot);
    println!("  spawns_daemons:      {}", st.spawns_daemons);
    println!("  control_plane_live: {}", st.control_plane_live);
    println!("  aos_home:           {}", home.display());
    println!(
        "  control_path:       {}",
        control_socket_path(home).display()
    );
    println!("  boot_order:         {}", DAEMON_BOOT_ORDER.join(" → "));
    println!(
        "  watchdogs_planned:  {} + {}",
        WATCHDOG_BASELINE.join(", "),
        WATCHDOG_P21_EXTRA.join(", ")
    );
    println!();
    println!("Next lots: P21.1 shared spawn lib → P21.2 headless tree (ADR 0012).");
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
                "aos-session spawns the tree (0.18 behaviour; no serverd)",
            ),
            SessionHandoff::BootstrapThenAttach => (
                "bootstrap-then-attach",
                "session ensures serverd is up, then attaches UI to the bus",
            ),
            SessionHandoff::AttachUiOnly => (
                "attach-ui-only",
                "serverd already owns the tree; session only launches egui",
            ),
        };
        println!("  {label:24} {desc}");
    }
}
