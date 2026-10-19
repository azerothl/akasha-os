//! Real DAP verification against an explicitly supplied installed Python/debugpy.
//! Runs only a generated fixture in a new temporary workspace; no user project.
use aos_platform::{
    workspace::WorkspaceBindManager,
    workspace_debug::{self, DebugManager, DebugRequest},
};
use serde_json::{json, Value};
fn request(value: Value) -> DebugRequest {
    serde_json::from_value(value).unwrap()
}
async fn stopped(manager: &mut DebugManager, id: &str, mut activity: Value) -> Result<u32, String> {
    for _ in 0..40 {
        let events = activity["events"]
            .as_array()
            .or_else(|| activity["activity"]["events"].as_array());
        if let Some(event) =
            events.and_then(|events| events.iter().find(|e| e["event"] == "stopped"))
        {
            return event["body"]["threadId"]
                .as_u64()
                .map(|id| id as u32)
                .ok_or("stopped event missing thread".into());
        }
        activity = manager
            .request(
                id,
                "probe",
                "fixture",
                &request(json!({"root":"fixture","command":"events"})),
            )
            .await?;
    }
    Err("fixture did not stop".into())
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let python = std::env::args()
        .nth(1)
        .ok_or("supply absolute Python executable with debugpy installed")?;
    let root = std::env::temp_dir().join(format!("aos-debug-proof-{}", std::process::id()));
    if root.exists() {
        return Err("proof directory already exists".into());
    }
    std::fs::create_dir_all(&root)?;
    std::fs::write(
        root.join("main.py"),
        "value = 40\nvalue += 2\nprint(value)\n",
    )?;
    let mut binds = WorkspaceBindManager::open(root.join("sessions"))?;
    binds.bind(&root.to_string_lossy(), Some("fixture"), "probe")?;
    let policy = root.join("workspace-debug-policy.json");
    std::fs::write(&policy,json!({"workspaces":{"fixture":{"python":{
        "adapter":{"program":python,"args":["-m","debugpy.adapter"]},"program":"main.py","args":[],"stop_on_entry":true}}}}).to_string())?;
    let caps = ["debug.run", "fs.read", "fs.write"]
        .iter()
        .map(|c| format!("{c}:/host/fixture/**"))
        .collect::<Vec<_>>();
    let prepared = workspace_debug::prepare(
        &binds,
        &policy,
        &request(json!({"root":"fixture","configuration":"python"})),
        &caps,
    )?;
    let mut manager = DebugManager::default();
    let start = manager.start(prepared, "probe").await?;
    let id = start["session_id"].as_str().ok_or("missing id")?.to_owned();
    let thread = stopped(&mut manager, &id, json!({})).await?;
    let mut threads = Vec::new();
    for _ in 0..2 {
        let step = manager
            .request(
                &id,
                "probe",
                "fixture",
                &request(json!({"root":"fixture","command":"next","thread_id":thread})),
            )
            .await?;
        threads.push(stopped(&mut manager, &id, step).await?);
    }
    let stack = manager
        .request(
            &id,
            "probe",
            "fixture",
            &request(json!({"root":"fixture","command":"stackTrace","thread_id":thread})),
        )
        .await?;
    let frame = stack["body"]["stackFrames"][0]["id"]
        .as_u64()
        .ok_or("missing frame")?;
    assert_eq!(
        stack["body"]["stackFrames"][0]["source"]["path"],
        "/host/fixture/main.py"
    );
    let scopes = manager
        .request(
            &id,
            "probe",
            "fixture",
            &request(json!({"root":"fixture","command":"scopes","frame_id":frame})),
        )
        .await?;
    let variables_ref = scopes["body"]["scopes"][0]["variablesReference"]
        .as_u64()
        .ok_or("missing locals")?;
    let variables = manager
        .request(
            &id,
            "probe",
            "fixture",
            &request(
                json!({"root":"fixture","command":"variables","variables_reference":variables_ref}),
            ),
        )
        .await?;
    assert!(
        variables["body"]["variables"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["name"] == "value" && v["value"] == "42"),
        "{variables}"
    );
    let denied = manager
        .request(
            &id,
            "another-actor",
            "fixture",
            &request(json!({"root":"fixture","command":"threads"})),
        )
        .await;
    assert!(denied.is_err());
    let stop = manager.stop(&id, "probe", "fixture").await?;
    println!(
        "{}",
        json!({"ok":true,"adapter":"debugpy","start":start,"step_stops":threads.len(),
        "stack":stack["body"],"variables":variables["body"],"wrong_actor_denied":true,"stop":stop})
    );
    assert!(root.starts_with(std::env::temp_dir()));
    std::fs::remove_dir_all(&root)?;
    Ok(())
}
