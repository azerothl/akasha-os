//! Named OpenAI-compatible providers persisted under `var/providers/` (P08.12).

use aos_proto::ProviderRecord;
use std::fs;
use std::path::PathBuf;

pub fn providers_dir() -> PathBuf {
    let home = std::env::var("AOS_HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join("var/providers")
}

pub fn presets() -> Vec<(&'static str, &'static str, Option<&'static str>)> {
    aos_proto::PROVIDER_PRESETS.to_vec()
}

pub fn load_all() -> Vec<ProviderRecord> {
    let dir = providers_dir();
    let mut out = Vec::new();
    let Ok(rd) = fs::read_dir(&dir) else {
        return out;
    };
    for ent in rd.flatten() {
        let path = ent.path();
        if path.extension().and_then(|e| e.to_str()) != Some("yaml") {
            continue;
        }
        if let Ok(raw) = fs::read_to_string(&path) {
            if let Ok(p) = serde_yaml::from_str::<ProviderRecord>(&raw) {
                out.push(p);
            }
        }
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

pub fn save(p: &ProviderRecord) -> Result<(), String> {
    if p.id.is_empty() {
        return Err("id provider vide".into());
    }
    let dir = providers_dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}.yaml", sanitize(&p.id)));
    let raw = serde_yaml::to_string(p).map_err(|e| e.to_string())?;
    fs::write(path, raw).map_err(|e| e.to_string())
}

pub fn remove(id: &str) -> Result<(), String> {
    let path = providers_dir().join(format!("{}.yaml", sanitize(id)));
    if path.exists() {
        fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn apply_preset(preset: &str) -> ProviderRecord {
    let (name, endpoint, secret) = presets()
        .into_iter()
        .find(|(n, _, _)| *n == preset)
        .unwrap_or(("custom", "", None));
    ProviderRecord {
        id: name.to_string(),
        preset: name.to_string(),
        endpoint: endpoint.to_string(),
        secret_name: secret.map(|s| s.to_string()),
        enabled: true,
        discovered_models: vec![],
    }
}

fn sanitize(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

pub fn endpoint_is_loopback(endpoint: &str) -> bool {
    let host = endpoint
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split(['/', ':'])
        .next()
        .unwrap_or("");
    matches!(host, "127.0.0.1" | "localhost" | "::1" | "[::1]")
}

pub async fn fetch_provider_secret(
    bus: &aos_ipc::BusClient,
    name: Option<&str>,
) -> Option<String> {
    let name = name.filter(|s| !s.is_empty())?;
    bus.call::<aos_proto::SecretGetRequest, String>(
        "secrets.get",
        &aos_proto::SecretGetRequest {
            name: name.to_string(),
            actor: String::new(),
        },
        vec![],
    )
    .await
    .ok()
}

pub async fn apply_provider_models(
    sub: &crate::ModelSubsystem,
    bus: &aos_ipc::BusClient,
    p: &aos_proto::ProviderRecord,
) {
    let key = fetch_provider_secret(bus, p.secret_name.as_deref()).await;
    let models = if p.discovered_models.is_empty() {
        vec!["default".into()]
    } else {
        p.discovered_models.clone()
    };
    for m in models {
        let id = format!("provider:{}:{}", p.id, m);
        sub.add_remote_backend(&id, &p.endpoint, &m, key.clone());
    }
}

/// Host/port from `http(s)://host[:port]/...`.
pub fn parse_host_port(endpoint: &str) -> (String, u16) {
    let without_scheme = endpoint
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    let hostport = without_scheme.split('/').next().unwrap_or("");
    match hostport.rsplit_once(':') {
        Some((h, p)) => (h.to_string(), p.parse().unwrap_or(443)),
        None => (
            hostport.to_string(),
            if endpoint.starts_with("https") {
                443
            } else {
                80
            },
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_presets() {
        assert!(endpoint_is_loopback("http://127.0.0.1:11434/v1"));
        assert!(!endpoint_is_loopback("https://api.openai.com/v1"));
    }

    #[test]
    fn parse_host_port_defaults() {
        assert_eq!(
            parse_host_port("http://127.0.0.1:11434/v1"),
            ("127.0.0.1".into(), 11434)
        );
        assert_eq!(
            parse_host_port("https://api.openai.com/v1"),
            ("api.openai.com".into(), 443)
        );
    }
}
