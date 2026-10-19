//! Read-only acceptance probe over a real repository through the production host.
//! Output and control state are created in a new directory, outside that repository.
use aos_platform::{
    intents::workspace_execution::dispatch, subsystem::PlatformConfig, workspace_search,
    PlatformSubsystem,
};
use aos_proto::workspace::FsSearchRequest;
use serde_json::json;
use std::path::{Path, PathBuf};
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 2 {
        return Err("usage: workspace_inspect_probe REAL_REPO NEW_OUTPUT_DIR".into());
    }
    let root = Path::new(&args[0]).canonicalize()?;
    let requested_output = PathBuf::from(&args[1]);
    if !requested_output.is_absolute() {
        return Err("proof output must be absolute".into());
    }
    // Normalize the existing parent before comparison, including Windows' \\?\
    // canonical prefix and any parent junctions. The leaf must not exist yet.
    let output = requested_output
        .parent()
        .ok_or("proof output parent missing")?
        .canonicalize()?
        .join(
            requested_output
                .file_name()
                .ok_or("proof output name missing")?,
        );
    if output.starts_with(&root) {
        return Err("proof output must be outside the inspected repository".into());
    }
    std::fs::create_dir(&output)?;
    let dir = |name: &str| output.join(name).to_string_lossy().into_owned();
    let config = PlatformConfig {
        bus: "ipc://proof".into(),
        audit_dir: dir("audit"),
        storage_dir: dir("storage"),
        memory_dir: dir("memory"),
        modules_dir: dir("modules"),
        catalogue_file: dir("absent.yaml"),
        community_catalogue_dir: dir("catalogue"),
        skills_dir: dir("skills"),
        sessions_dir: dir("sessions"),
        embed_model: None,
        policies_file: None,
        confirm_timeout_sec: 10,
        secrets_file: dir("secrets/store"),
        net_mode: "offline_strict".into(),
        memory_v2: false,
        memory_v2_shadow: false,
    };
    let sub = PlatformSubsystem::open(&config)?;
    sub.workspaces
        .lock()
        .unwrap()
        .bind(&root.to_string_lossy(), Some("realrepo"), "human:ui")?;
    let caps = vec!["fs.read:/host/realrepo/**".into()];
    let begun = std::time::Instant::now();
    let inventory = dispatch(
        &sub,
        "workspace.inspect",
        json!({"root":"realrepo"}),
        "human:ui",
        &caps,
        "real-repo-inspect",
    )
    .await?;
    let inspect_ms = begun.elapsed().as_millis();
    std::fs::write(
        output.join("inventory.json"),
        serde_json::to_vec_pretty(&inventory)?,
    )?;
    let response = {
        let mgr = sub.workspaces.lock().unwrap();
        workspace_search::search_workspace(
            &mgr,
            &FsSearchRequest {
                root: "realrepo".into(),
                query: "workspace.bind".into(),
                glob: None,
                limit: 20,
                case_sensitive: true,
                actor: "human:ui".into(),
                caps: caps.clone(),
                trace_id: "real-repo-search".into(),
            },
        )
    };
    if !response.ok || response.hits.is_empty() {
        return Err("real repository search failed or returned no expected references".into());
    }
    drop(sub);
    let restarted = PlatformSubsystem::open(&config)?;
    let cached = dispatch(
        &restarted,
        "workspace.inventory",
        json!({"root":"realrepo"}),
        "human:ui",
        &caps,
        "reload-inventory",
    )
    .await?;
    assert_eq!(cached["content_version"], inventory["content_version"]);
    assert_eq!(cached["cached"], true);
    let denied = dispatch(
        &restarted,
        "workspace.inspect",
        json!({"root":"realrepo"}),
        "agent:ungranted",
        &[],
        "denied-inspect",
    )
    .await
    .is_err();
    assert!(denied);
    let report = json!({"ok":true,"scope":"actual read-only repository; no source writes or embeddings quality claim","fixture":output,"inspection_ms":inspect_ms,"scan":inventory["scan"],"content_version":inventory["content_version"],"source_files":inventory["files"].as_array().unwrap().len(),"search_hits":response.hits.len(),"search_truncated":response.truncated,"cache_survived_host_restart":true,"ungranted_inspection_denied":denied});
    std::fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{report}");
    Ok(())
}
