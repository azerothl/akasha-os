//! Retours testeurs Preview (F-SEC-03 / offline-first).
//!
//! Toujours écrit en local. Publication GitHub : explicite (`publish_github`),
//! jamais pour `security` (rapport public interdit).

use aos_proto::{FeedbackSubmitRequest, FeedbackSubmitResponse};
use crate::net::EgressControl;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Dépôt public des issues Preview.
pub const GITHUB_REPO: &str = "azerothl/akasha-os";

/// Écrit un retour dans `feedback_dir/<id>.json` (pas d'envoi réseau).
pub fn submit(
    feedback_dir: impl AsRef<Path>,
    req: FeedbackSubmitRequest,
) -> Result<FeedbackSubmitResponse, String> {
    let dir = feedback_dir.as_ref();
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let id = format!("fb-{ts}");
    let path = dir.join(format!("{id}.json"));

    let mut doc = serde_json::json!({
        "id": id,
        "title": req.title,
        "category": req.category,
        "severity": req.severity,
        "body": req.body,
        "scenario": req.scenario,
        "meta": req.meta,
        "ts_ms": ts,
    });
    // Sanitize : jamais de secrets dans le fichier.
    if let Some(obj) = doc.get_mut("meta").and_then(|m| m.as_object_mut()) {
        for k in ["api_key", "secret", "password", "token"] {
            obj.remove(k);
        }
    }

    let pretty = serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?;
    fs::write(&path, pretty).map_err(|e| e.to_string())?;

    // Dossier d'export (même contenu ; le zip est laissé à l'UI / OS).
    let export_dir = dir.join(&id);
    fs::create_dir_all(&export_dir).map_err(|e| e.to_string())?;
    let export_json = export_dir.join("feedback.json");
    fs::copy(&path, &export_json).map_err(|e| e.to_string())?;
    let _ = fs::write(
        export_dir.join("README.txt"),
        "Akasha OS Preview — paquet de retour testeur.\n\
         Une copie locale est toujours conservée ici.\n\
         Si vous avez coché « Créer une issue GitHub », une issue (ou un formulaire)\n\
         a été ouverte sur https://github.com/azerothl/akasha-os/issues.\n",
    );

    Ok(FeedbackSubmitResponse {
        id,
        path: path_str(&path),
        export_dir: path_str(&export_dir),
        github_issue_url: None,
        github_issue_number: None,
        github_status: "local_only".into(),
    })
}

pub fn github_repo() -> String {
    std::env::var("AOS_GITHUB_REPO").unwrap_or_else(|_| GITHUB_REPO.into())
}

fn is_security(category: &str) -> bool {
    category.eq_ignore_ascii_case("security")
}

pub fn is_security_category(category: &str) -> bool {
    is_security(category)
}

fn labels_for_category(category: &str) -> Vec<&'static str> {
    match category.to_ascii_lowercase().as_str() {
        "bug" => vec!["bug"],
        "ux" | "perf" | "other" => vec!["enhancement"],
        _ => vec![],
    }
}

pub fn issue_title(req: &FeedbackSubmitRequest) -> String {
    let cat = req.category.trim();
    let title = req.title.trim();
    if cat.is_empty() {
        format!("[Preview] {title}")
    } else {
        format!("[Preview][{cat}] {title}")
    }
}

pub fn issue_body(req: &FeedbackSubmitRequest, local_id: &str) -> String {
    let scenario = req.scenario.as_deref().unwrap_or("—");
    let meta = serde_json::to_string_pretty(&req.meta).unwrap_or_else(|_| "{}".into());
    format!(
        "## Rapport Preview\n\
         \n\
         - **Catégorie :** {cat}\n\
         - **Sévérité :** {sev}\n\
         - **Scénario :** {scenario}\n\
         - **Id local :** `{local_id}`\n\
         \n\
         ## Description\n\
         \n\
         {body}\n\
         \n\
         ## Méta\n\
         \n\
         ```json\n{meta}\n```\n",
        cat = req.category,
        sev = req.severity,
        body = req.body.trim(),
    )
}

fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// URL du formulaire GitHub prérempli (aucun jeton requis).
pub fn new_issue_form_url(req: &FeedbackSubmitRequest, local_id: &str) -> String {
    let repo = github_repo();
    let title = issue_title(req);
    let mut body = issue_body(req, local_id);
    const MAX_BODY: usize = 1200;
    if body.len() > MAX_BODY {
        body.truncate(MAX_BODY);
        body.push_str("\n\n_(corps tronqué — voir le fichier local `var/feedback/`)_\n");
    }
    let mut url = format!(
        "https://github.com/{repo}/issues/new?title={}&body={}",
        percent_encode(&title),
        percent_encode(&body)
    );
    if let Some(label) = labels_for_category(&req.category).first() {
        url.push_str("&labels=");
        url.push_str(percent_encode(label).as_str());
    }
    url
}

#[derive(Debug)]
pub struct GithubPublish {
    pub issue_url: String,
    pub issue_number: Option<u64>,
    pub via: &'static str,
}

fn github_caps() -> Vec<String> {
    vec![
        "net.connect:api.github.com:443".into(),
        "net.connect:github.com:443".into(),
    ]
}

fn try_gh_cli(req: &FeedbackSubmitRequest, local_id: &str) -> Result<GithubPublish, String> {
    let repo = github_repo();
    let title = issue_title(req);
    let body = issue_body(req, local_id);
    let mut cmd = std::process::Command::new("gh");
    cmd.args(["issue", "create", "--repo", &repo, "--title", &title, "--body", &body]);
    for label in labels_for_category(&req.category) {
        cmd.args(["--label", label]);
    }
    let out = cmd.output().map_err(|e| format!("gh: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("gh: {}", err.trim()));
    }
    let url = String::from_utf8_lossy(&out.stdout)
        .lines()
        .rev()
        .find(|l| l.contains("github.com/") && l.contains("/issues/"))
        .unwrap_or("")
        .trim()
        .to_string();
    if url.is_empty() {
        return Err("gh: pas d'URL d'issue dans la sortie".into());
    }
    let number = url
        .rsplit('/')
        .next()
        .and_then(|n| n.parse().ok());
    Ok(GithubPublish {
        issue_url: url,
        issue_number: number,
        via: "gh",
    })
}

fn try_github_api(
    net: &mut EgressControl,
    token: &str,
    req: &FeedbackSubmitRequest,
    local_id: &str,
) -> Result<GithubPublish, String> {
    let caps = github_caps();
    if !net.check("human:ui", "api.github.com", 443, &caps) {
        return Err("egress api.github.com refusé (activez le réseau)".into());
    }
    let repo = github_repo();
    let labels: Vec<String> = labels_for_category(&req.category)
        .into_iter()
        .map(str::to_string)
        .collect();
    let payload = serde_json::json!({
        "title": issue_title(req),
        "body": issue_body(req, local_id),
        "labels": labels,
    });
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .post(format!("https://api.github.com/repos/{repo}/issues"))
        .header("Accept", "application/vnd.github+json")
        .header("Authorization", format!("Bearer {token}"))
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "akasha-os-preview")
        .json(&payload)
        .send()
        .map_err(|e| e.to_string())?;
    let status = resp.status();
    let v: serde_json::Value = resp.json().map_err(|e| e.to_string())?;
    if !status.is_success() {
        let msg = v
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("erreur GitHub");
        return Err(format!("GitHub API {status}: {msg}"));
    }
    let number = v.get("number").and_then(|n| n.as_u64());
    let url = v
        .get("html_url")
        .and_then(|u| u.as_str())
        .map(str::to_string)
        .ok_or_else(|| "réponse GitHub sans html_url".to_string())?;
    Ok(GithubPublish {
        issue_url: url,
        issue_number: number,
        via: "api",
    })
}

/// Publie une issue (API / `gh`) ou renvoie l'URL du formulaire.
pub fn publish_to_github(
    net: &mut EgressControl,
    token: Option<&str>,
    req: &FeedbackSubmitRequest,
    local_id: &str,
) -> Result<GithubPublish, String> {
    if is_security(&req.category) {
        return Err("skipped_security".into());
    }
    let online = !matches!(net.mode(), crate::net::NetMode::OfflineStrict);
    if online {
        if let Ok(p) = try_gh_cli(req, local_id) {
            return Ok(p);
        }
        if let Some(t) = token.filter(|s| !s.is_empty()) {
            return try_github_api(net, t, req, local_id);
        }
    }
    Ok(GithubPublish {
        issue_url: new_issue_form_url(req, local_id),
        issue_number: None,
        via: "form",
    })
}

fn path_str(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

/// Chemin feedback sous AOS_HOME ou `var/feedback`.
pub fn default_dir() -> PathBuf {
    if let Ok(h) = std::env::var("AOS_HOME") {
        PathBuf::from(h).join("var/feedback")
    } else {
        PathBuf::from("var/feedback")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aos_proto::FeedbackSubmitRequest;

    #[test]
    fn submit_ecrit_json_sans_secret() {
        let dir = std::env::temp_dir().join(format!(
            "aos-fb-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        let resp = submit(
            &dir,
            FeedbackSubmitRequest {
                title: "test".into(),
                category: "ux".into(),
                severity: "low".into(),
                body: "hello".into(),
                scenario: Some("chat_offline".into()),
                meta: serde_json::json!({ "api_key": "secret", "os": "win" }),
                publish_github: false,
            },
        )
        .unwrap();
        let raw = fs::read_to_string(&resp.path).unwrap();
        assert!(raw.contains("hello"));
        assert!(!raw.contains("secret"));
        assert!(raw.contains("\"os\": \"win\""));
        assert_eq!(resp.github_status, "local_only");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn formulaire_github_encode_le_titre() {
        let req = FeedbackSubmitRequest {
            title: "ça casse & tout".into(),
            category: "bug".into(),
            severity: "high".into(),
            body: "repro".into(),
            scenario: None,
            meta: serde_json::json!({}),
            publish_github: true,
        };
        let url = new_issue_form_url(&req, "fb-1");
        assert!(url.starts_with("https://github.com/azerothl/akasha-os/issues/new?"));
        assert!(url.contains("labels=bug"));
        assert!(url.contains("Preview"));
    }

    #[test]
    fn security_n_est_pas_publie() {
        let req = FeedbackSubmitRequest {
            title: "leak".into(),
            category: "security".into(),
            severity: "high".into(),
            body: "x".into(),
            scenario: None,
            meta: serde_json::json!({}),
            publish_github: true,
        };
        let mut net = crate::net::EgressControl::new();
        let err = publish_to_github(&mut net, None, &req, "fb-1").unwrap_err();
        assert_eq!(err, "skipped_security");
    }
}
