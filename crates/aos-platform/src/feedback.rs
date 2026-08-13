//! Retours testeurs Preview (local only, F-SEC-03 / offline-first).

use aos_proto::{FeedbackSubmitRequest, FeedbackSubmitResponse};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

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
        "Agent OS Preview — paquet de retour testeur.\n\
         Joindre ce dossier (ou feedback.json) à une issue GitHub / canal cohorte.\n\
         Aucune donnée n'a été envoyée automatiquement.\n",
    );

    Ok(FeedbackSubmitResponse {
        id,
        path: path_str(&path),
        export_dir: path_str(&export_dir),
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
            },
        )
        .unwrap();
        let raw = fs::read_to_string(&resp.path).unwrap();
        assert!(raw.contains("hello"));
        assert!(!raw.contains("secret"));
        assert!(raw.contains("\"os\": \"win\""));
        let _ = fs::remove_dir_all(&dir);
    }
}
