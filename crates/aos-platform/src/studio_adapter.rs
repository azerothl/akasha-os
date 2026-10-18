//! Optional, project-scoped adapter to the existing Akasha Code Studio daemon.
//! No arbitrary URLs, project IDs or daemon commands are accepted from guests.
use crate::workspace::WorkspaceBindManager;
use reqwest::Url;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    daemon_url: String,
    ui_url: String,
    workspaces: HashMap<String, String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub root: String,
    pub action: Option<String>,
    pub path: Option<String>,
    pub text: Option<String>,
    pub position: Option<Position>,
}

#[derive(Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

pub struct Prepared {
    origin: Url,
    ui: Url,
    project: String,
    host: PathBuf,
    pub workspace_id: String,
    operation: String,
    body: Option<Value>,
}

fn loopback_origin(raw: &str) -> Result<Url, String> {
    let url = Url::parse(raw).map_err(|_| "invalid Studio URL")?;
    // Numeric addresses avoid DNS rebinding. The adapter is local and opt-in.
    if url.scheme() != "http"
        || !matches!(url.host_str(), Some("127.0.0.1" | "[::1]"))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("Studio URLs must be numeric HTTP loopback origins".into());
    }
    Ok(url)
}

pub fn prepare(
    mgr: &WorkspaceBindManager,
    policy: &Path,
    intent: &str,
    request: Request,
    caps: &[String],
) -> Result<Prepared, String> {
    let id = request
        .root
        .strip_prefix("/host/")
        .unwrap_or(&request.root)
        .trim_end_matches('/');
    if id.is_empty()
        || id.contains('/')
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err("Studio requires a bound workspace root".into());
    }
    let required = format!("fs.read:/host/{id}/**");
    if !caps.contains(&required) {
        return Err(format!("required capability: {required}"));
    }
    if intent != "studio.project" {
        let required = format!("{intent}:/host/{id}/**");
        if !caps.contains(&required) {
            return Err(format!("required capability: {required}"));
        }
    }
    if intent == "studio.reindex" && !caps.contains(&format!("fs.write:/host/{id}/**")) {
        return Err("Studio reindex requires workspace write capability".into());
    }
    let host = PathBuf::from(mgr.resolve_host_path(id).ok_or("workspace is not bound")?)
        .canonicalize()
        .map_err(|_| "workspace unavailable")?;
    let config: Config = serde_json::from_slice(
        &std::fs::read(policy)
            .map_err(|_| "Code Studio adapter is disabled: configure studio-adapter.json")?,
    )
    .map_err(|e| format!("invalid Studio adapter policy: {e}"))?;
    let project = config
        .workspaces
        .get(id)
        .ok_or("workspace is not mapped to a Studio project")?
        .clone();
    if project.is_empty()
        || project.len() > 128
        || !project
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err("invalid configured Studio project id".into());
    }
    let body = match intent {
        "studio.project" => None,
        "studio.reindex" => Some(json!({"force": false})),
        "studio.lsp" => {
            let action = request.action.as_deref().ok_or("LSP action required")?;
            if !matches!(action, "diagnostics" | "definition" | "references") {
                return Err("unsupported read-only LSP action".into());
            }
            let path = request
                .path
                .as_deref()
                .ok_or("relative LSP path required")?;
            if path.is_empty()
                || path.contains(['\\', ':', '\0'])
                || path
                    .split('/')
                    .any(|p| p.is_empty() || p == "." || p == "..")
            {
                return Err("invalid relative LSP path".into());
            }
            let resolved = host
                .join(path)
                .canonicalize()
                .map_err(|_| "LSP file unavailable")?;
            if !resolved.starts_with(&host) || !resolved.is_file() {
                return Err("LSP path escapes workspace".into());
            }
            let text = request.text.unwrap_or_default();
            if text.len() > 256 * 1024 {
                return Err("LSP text exceeds 256 KiB".into());
            }
            if action != "diagnostics" && request.position.is_none() {
                return Err("LSP position required".into());
            }
            Some(json!({"action":action,"path":path,"text":text,"position":request.position}))
        }
        _ => return Err("unsupported Studio operation".into()),
    };
    Ok(Prepared {
        origin: loopback_origin(&config.daemon_url)?,
        ui: loopback_origin(&config.ui_url)?,
        project,
        host,
        workspace_id: id.into(),
        operation: intent.into(),
        body,
    })
}

async fn read_json(response: reqwest::Response) -> Result<Value, String> {
    let mut response = response
        .error_for_status()
        .map_err(|e| format!("Studio request failed: {e}"))?;
    let mut data = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        if data.len() + chunk.len() > 2 * 1024 * 1024 {
            return Err("Studio response exceeds 2 MiB".into());
        }
        data.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&data).map_err(|e| format!("invalid Studio response: {e}"))
}

impl Prepared {
    pub async fn execute(self) -> Result<Value, String> {
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| e.to_string())?;
        // Resolve identity on every operation, before sending any file contents.
        let projects = read_json(
            client
                .get(self.origin.join("api/studio/projects").unwrap())
                .send()
                .await
                .map_err(|e| e.to_string())?,
        )
        .await?;
        let project = projects["projects"]
            .as_array()
            .and_then(|p| p.iter().find(|p| p["id"].as_str() == Some(&self.project)))
            .ok_or("mapped project is missing from Code Studio")?;
        let actual = project["path"]
            .as_str()
            .ok_or("Studio project path missing")?;
        if PathBuf::from(actual)
            .canonicalize()
            .map_err(|_| "Studio project path unavailable")?
            != self.host
        {
            return Err("Studio project path differs from the bound workspace".into());
        }
        let base = format!("api/studio/projects/{}", self.project);
        if self.operation == "studio.project" {
            let metadata = read_json(
                client
                    .get(self.origin.join(&base).unwrap())
                    .send()
                    .await
                    .map_err(|e| e.to_string())?,
            )
            .await?;
            let index = read_json(
                client
                    .get(
                        self.origin
                            .join(&format!("{base}/code-rag/status"))
                            .unwrap(),
                    )
                    .send()
                    .await
                    .map_err(|e| e.to_string())?,
            )
            .await?;
            // Current Studio selects projects in its own load menu; don't invent a deep link.
            return Ok(
                json!({"ok":true,"workspace_id":self.workspace_id,"project_id":self.project,
                "ui_url":self.ui.as_str(),"project":metadata,"code_rag":index,
                "handoff":"Open Code Studio and select this project in Load project",
                "rows":[{"field":"project_id","value":self.project},
                    {"field":"ui_url","value":self.ui.as_str()},
                    {"field":"code_index","value":index["status"]},
                    {"field":"project_summary","value":metadata["project_summary"]},
                    {"field":"evolution_summary","value":metadata["evolution_summary"]}]}),
            );
        }
        let suffix = if self.operation == "studio.lsp" {
            "lsp/query"
        } else {
            "code-rag/reindex"
        };
        read_json(
            client
                .post(self.origin.join(&format!("{base}/{suffix}")).unwrap())
                .json(&self.body)
                .send()
                .await
                .map_err(|e| e.to_string())?,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    struct Fixture {
        root: PathBuf,
        mgr: WorkspaceBindManager,
        policy: PathBuf,
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
    impl Fixture {
        fn new(origin: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "aos-studio-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(root.join("project")).unwrap();
            std::fs::write(root.join("project/main.rs"), "fn main() {}\n").unwrap();
            let mut mgr = WorkspaceBindManager::open(root.join("sessions")).unwrap();
            mgr.bind(
                &root.join("project").to_string_lossy(),
                Some("demo"),
                "test",
            )
            .unwrap();
            let policy = root.join("studio-adapter.json");
            std::fs::write(
                &policy,
                json!({"daemon_url":origin,"ui_url":"http://127.0.0.1:5178",
                "workspaces":{"demo":"project-1"}})
                .to_string(),
            )
            .unwrap();
            Self { root, mgr, policy }
        }
        fn request(&self) -> Request {
            serde_json::from_value(json!({"root":"demo"})).unwrap()
        }
    }

    async fn fixture_server(
        replies: Vec<(&'static str, Value)>,
    ) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let job = tokio::spawn(async move {
            for (expected, body) in replies {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                loop {
                    let mut byte = [0u8; 1];
                    socket.read_exact(&mut byte).await.unwrap();
                    bytes.push(byte[0]);
                    if bytes.ends_with(b"\r\n\r\n") {
                        break;
                    }
                    assert!(bytes.len() < 8192);
                }
                assert!(String::from_utf8(bytes).unwrap().starts_with(expected));
                let body = body.to_string();
                let response = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",body.len());
                socket.write_all(response.as_bytes()).await.unwrap();
            }
        });
        (origin, job)
    }

    #[tokio::test]
    async fn reads_real_http_contract_and_rejects_a_different_project_root() {
        let f = Fixture::new("http://127.0.0.1:1");
        let projects = json!({"projects":[{"id":"project-1","path":f.root.join("project")}]});
        let (origin, server) = fixture_server(vec![
            ("GET /api/studio/projects ", projects),
            (
                "GET /api/studio/projects/project-1 ",
                json!({"id":"project-1","evolution_summary":"remembered project context"}),
            ),
            (
                "GET /api/studio/projects/project-1/code-rag/status ",
                json!({"status":"ready","files_indexed":1}),
            ),
        ])
        .await;
        let mut config: Value = serde_json::from_slice(&std::fs::read(&f.policy).unwrap()).unwrap();
        config["daemon_url"] = json!(origin);
        std::fs::write(&f.policy, config.to_string()).unwrap();
        let result = prepare(
            &f.mgr,
            &f.policy,
            "studio.project",
            f.request(),
            &["fs.read:/host/demo/**".into()],
        )
        .unwrap()
        .execute()
        .await
        .unwrap();
        assert_eq!(result["code_rag"]["status"], "ready");
        assert_eq!(
            result["project"]["evolution_summary"],
            "remembered project context"
        );
        server.await.unwrap();
        let (origin, server) = fixture_server(vec![(
            "GET /api/studio/projects ",
            json!({"projects":[{"id":"project-1","path":f.root}]}),
        )])
        .await;
        config["daemon_url"] = json!(origin);
        std::fs::write(&f.policy, config.to_string()).unwrap();
        let error = prepare(
            &f.mgr,
            &f.policy,
            "studio.project",
            f.request(),
            &["fs.read:/host/demo/**".into()],
        )
        .unwrap()
        .execute()
        .await
        .unwrap_err();
        assert!(error.contains("differs"));
        server.await.unwrap();
    }

    #[test]
    fn grants_and_lsp_paths_fail_closed_before_http() {
        let f = Fixture::new("http://127.0.0.1:1");
        assert!(prepare(&f.mgr, &f.policy, "studio.project", f.request(), &[]).is_err());
        assert!(prepare(
            &f.mgr,
            &f.policy,
            "studio.reindex",
            f.request(),
            &["fs.read:/host/demo/**".into()]
        )
        .is_err());
        let caps = vec![
            "fs.read:/host/demo/**".into(),
            "studio.lsp:/host/demo/**".into(),
        ];
        for path in [
            "../outside",
            "C:/secret",
            "main.rs/../../outside",
            "/main.rs",
        ] {
            let req =
                serde_json::from_value(json!({"root":"demo","action":"diagnostics","path":path}))
                    .unwrap();
            assert!(prepare(&f.mgr, &f.policy, "studio.lsp", req, &caps).is_err());
        }
        let req = serde_json::from_value(
            json!({"root":"demo","action":"diagnostics","path":"main.rs","text":"fn main() {}"}),
        )
        .unwrap();
        assert!(prepare(&f.mgr, &f.policy, "studio.lsp", req, &caps).is_ok());
    }
    #[test]
    fn only_numeric_loopback_origins_are_accepted() {
        for url in ["http://127.0.0.1:3876", "http://[::1]:3876/"] {
            assert!(loopback_origin(url).is_ok());
        }
        for url in [
            "https://example.org",
            "http://localhost:3876",
            "http://127.0.0.1/api",
            "http://user@127.0.0.1",
            "http://127.0.0.1?x=1",
        ] {
            assert!(loopback_origin(url).is_err(), "{url}");
        }
    }
}
