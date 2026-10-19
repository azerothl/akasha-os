//! Real adapter -> Akasha daemon -> project persistence/index/LSP verification.
//! Refuses user projects: the orchestration fixture must create its marker first.
use aos_platform::{studio_adapter, workspace::WorkspaceBindManager};
use serde_json::{json, Value};
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 3 {
        return Err("provide daemon origin, fixture project id, and fixture path".into());
    }
    let root = std::path::PathBuf::from(&args[2]).canonicalize()?;
    if !root.join(".akasha-os-adapter-fixture").is_file() {
        return Err("fixture marker missing; refusing to update a user project".into());
    }
    let home = std::env::temp_dir().join(format!("aos-studio-probe-{}", std::process::id()));
    std::fs::create_dir_all(&home)?;
    let mut mgr = WorkspaceBindManager::open(&home)?;
    mgr.bind(&root.to_string_lossy(), Some("fixture"), "probe")?;
    let policy = home.join("studio-adapter.json");
    std::fs::write(
        &policy,
        json!({"daemon_url":args[0],"ui_url":"http://127.0.0.1:5178",
        "workspaces":{"fixture":args[1]}})
        .to_string(),
    )?;
    let caps = [
        "fs.read",
        "fs.write",
        "studio.reindex",
        "studio.memory.update",
        "studio.lsp",
    ]
    .iter()
    .map(|c| format!("{c}:/host/fixture/**"))
    .collect::<Vec<_>>();
    let invoke = |intent: &str, mut request: Value| {
        request["root"] = json!("fixture");
        studio_adapter::prepare(
            &mgr,
            &policy,
            intent,
            serde_json::from_value(request).unwrap(),
            &caps,
        )
    };
    let before = invoke("studio.project", json!({}))?.execute().await?;
    let update = invoke(
        "studio.memory.update",
        json!({"field":"project_summary","text":"Akasha OS fixture: project memory stays scoped"}),
    )?
    .execute()
    .await?;
    let memory = invoke("studio.memory", json!({}))?.execute().await?;
    assert_eq!(
        memory["project"]["project_summary"],
        "Akasha OS fixture: project memory stays scoped"
    );
    assert_eq!(memory["namespace"], format!("studio-project:{}", args[1]));
    let index = invoke("studio.reindex", json!({}))?.execute().await?;
    let definitions = invoke(
        "studio.lsp",
        json!({"action":"definition","path":"main.ts","position":{"line":1,"character":16}}),
    )?
    .execute()
    .await?;
    assert!(
        !definitions["result"]["locations"]
            .as_array()
            .ok_or_else(|| format!("missing LSP locations: {definitions}"))?
            .is_empty(),
        "{definitions}"
    );
    let mut stale: Value = serde_json::from_slice(&std::fs::read(&policy)?)?;
    stale["workspaces"]["fixture"] = json!("00000000-0000-0000-0000-000000000000");
    std::fs::write(&policy, stale.to_string())?;
    let refused = invoke("studio.project", json!({}))?
        .execute()
        .await
        .is_err();
    assert!(refused);
    println!(
        "{}",
        json!({"ok":true,"project":args[1],"before_index":before["code_rag"],
        "memory_update":update,"memory":memory["project"]["project_summary"],"index":index,
        "definitions":definitions,"unknown_project_denied":refused})
    );
    std::fs::remove_dir_all(&home)?;
    Ok(())
}
