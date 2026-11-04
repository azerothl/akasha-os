//! Native bridge -> broker -> production host, including denied confirmation.
//! The only administrator task reruns the harmless helper test in this executable.
use aos_ipc::{BusClient, BusService};
use aos_platform::{
    intents::{workspace, workspace_execution},
    subsystem::PlatformConfig,
    PlatformSubsystem,
};
use serde_json::{json, Value};
use std::{path::PathBuf, time::Duration};

#[test]
fn helper_process() {
    println!("native-agent-task-verified");
}

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        // The absolute recursive deletion target must remain in our temp prefix.
        let Ok(path) = self.0.canonicalize() else {
            return;
        };
        let Ok(temp) = std::env::temp_dir().canonicalize() else {
            return;
        };
        if path.parent() == Some(temp.as_path())
            && path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .starts_with("aos-native-agent-test-")
        {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

#[tokio::test]
async fn edit_confirmed_task_and_undo_keep_host_authority() {
    let home = Fixture(std::env::temp_dir().join(format!(
        "aos-native-agent-test-{:032x}",
        rand::random::<u128>()
    )));
    let project = home.0.join("project");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(project.join("one.txt"), "one").unwrap();
    std::fs::write(project.join("two.txt"), "two").unwrap();
    let dir = |name: &str| home.0.join(name).to_string_lossy().into_owned();
    let config = PlatformConfig {
        bus: "fixture".into(),
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
        confirm_timeout_sec: 5,
        secrets_file: dir("secrets/store"),
        net_mode: "online".into(),
        memory_v2: false,
        memory_v2_shadow: false,
    };
    let sub = PlatformSubsystem::open(&config).unwrap();
    sub.workspaces
        .lock()
        .unwrap()
        .bind(&project.to_string_lossy(), Some("demo"), "human:ui")
        .unwrap();
    std::fs::write(home.0.join("sessions/workspace-process-policy.json"), json!({"workspaces":{"demo":{
        "check":{"program":std::env::current_exe().unwrap(),"args":["--exact","helper_process","--nocapture"],"timeout_sec":10}
    }}}).to_string()).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let broker = tokio::spawn(aos_ipc::broker::serve(listener));
    let mut svc = BusService::new("native-agent-test-host");
    workspace::register(&mut svc, sub.clone());
    workspace_execution::register(&mut svc, sub.clone());
    let service_addr = addr.clone();
    let service = tokio::spawn(async move { svc.serve(&service_addr).await });
    let bus = BusClient::connect(&addr, "agent:native-proof")
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while !bus.lookup("process.run").await.unwrap() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let read_only = vec!["fs.read:/host/demo/**".to_owned()];
    let caps = ["fs.read", "fs.write", "process.run"]
        .map(|kind| format!("{kind}:/host/demo/**"))
        .to_vec();
    let patch = json!({"hunks":[{"path":"/host/demo/one.txt","diff":"new one","replace_all":true},{"path":"/host/demo/two.txt","diff":"new two","replace_all":true}]});
    let invoke = |name: &'static str, args: Value, caps: Vec<String>| {
        let bus = bus.clone();
        async move {
            serde_json::from_str::<Value>(
                &aos_agent::workspace_operations::invoke(&bus, "native-proof", &caps, name, &args)
                    .await,
            )
            .unwrap()
        }
    };
    assert_eq!(
        invoke("fs.apply_patch", patch.clone(), read_only.clone()).await["ok"],
        false
    );
    assert_eq!(
        std::fs::read_to_string(project.join("one.txt")).unwrap(),
        "one"
    );
    let applied = invoke("fs.apply_patch", patch, caps.clone()).await;
    assert_eq!(applied["ok"], true, "{applied}");
    assert_eq!(applied["applied"].as_array().unwrap().len(), 2);
    assert_eq!(
        std::fs::read_to_string(project.join("two.txt")).unwrap(),
        "new two"
    );
    let task = json!({"root":"demo","task":"check"});
    let file_caps = caps
        .iter()
        .filter(|cap| !cap.starts_with("process.run:"))
        .cloned()
        .collect();
    let denied = invoke("process.run", task.clone(), file_caps).await;
    assert_eq!(denied["ok"], false);
    let confirm = sub.confirm.clone();
    let mut pending = confirm.subscribe().await;
    let approvals = tokio::spawn(async move {
        for approved in [false, true] {
            let request = tokio::time::timeout(Duration::from_secs(5), pending.recv())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(request.actor, "agent:native-proof");
            assert_eq!(request.target, "demo");
            assert_eq!(request.action, "process.run");
            confirm.respond(&request.id, approved, false).await;
        }
    });
    let denied = invoke("process.run", task.clone(), caps.clone()).await;
    assert_eq!(denied["ok"], false);
    assert!(denied["error"]
        .as_str()
        .unwrap()
        .contains("confirmation denied"));
    let executed = invoke("process.run", task, caps.clone()).await;
    assert_eq!(executed["ok"], true, "{executed}");
    assert_eq!(executed["exit_code"], 0);
    assert!(executed["stdout"]
        .as_str()
        .unwrap()
        .contains("native-agent-task-verified"));
    approvals.await.unwrap();
    let undo = json!({"undo_group_id":applied["undo_group_id"]});
    assert_eq!(
        invoke("fs.undo_patch", undo.clone(), read_only).await["ok"],
        false
    );
    assert_eq!(invoke("fs.undo_patch", undo, caps).await["ok"], true);
    assert_eq!(
        std::fs::read_to_string(project.join("one.txt")).unwrap(),
        "one"
    );
    assert_eq!(
        std::fs::read_to_string(project.join("two.txt")).unwrap(),
        "two"
    );
    assert!(sub.audit.lock().unwrap().verify().is_ok());
    let audit = std::fs::read_to_string(home.0.join("audit/audit.jsonl")).unwrap();
    for term in [
        "agent:native-proof",
        "fs.apply_patch",
        "process.run",
        "fs.undo_patch",
        "confirmation denied",
    ] {
        assert!(audit.contains(term));
    }
    service.abort();
    broker.abort();
}
