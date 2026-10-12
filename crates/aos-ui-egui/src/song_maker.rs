//! Native, opt-in client for Song Maker's loopback DeclUI host (#343).
//!
//! This client intentionally consumes only the capabilities advertised by the
//! host. At present that is `list_projects`; generation and export stay in
//! Song Maker itself.

use reqwest::{Url, blocking::Client};
use serde_json::{Value, json};
use std::net::IpAddr;
use std::time::Duration;

const API_VERSION: &str = "song-maker.host.v1";
const MUSIC_API_ID: &str = "song-maker-music";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SongMakerProject {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) updated_at: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct SongMakerConnection {
    pub(crate) endpoint: String,
    pub(crate) api_version: String,
    pub(crate) capabilities: Vec<String>,
    pub(crate) projects: Vec<SongMakerProject>,
    pub(crate) projects_error: Option<String>,
}

/// Discover the configured local host and, when advertised, load its projects.
/// HTTP requests have a short timeout and are called from a worker thread.
pub(crate) fn connect_and_list(endpoint: &str) -> Result<SongMakerConnection, String> {
    let base = validate_endpoint(endpoint)?;
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(4))
        .build()
        .map_err(|_| "Impossible de préparer la connexion locale.".to_string())?;

    let discovery_url = base
        .join("/v1/host/discover")
        .map_err(|_| "Adresse locale Song Maker invalide.".to_string())?;
    let response = client
        .get(discovery_url)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .map_err(|_| "Song Maker ne répond pas à cette adresse locale.".to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "La découverte Song Maker a répondu HTTP {}.",
            response.status().as_u16()
        ));
    }
    let body = response
        .bytes()
        .map_err(|_| "La réponse de découverte Song Maker est illisible.".to_string())?;
    let discovery: Value = serde_json::from_slice(&body)
        .map_err(|_| "La réponse de découverte Song Maker n’est pas un JSON valide.".to_string())?;
    let (api_version, capabilities) = parse_discovery(&discovery)?;

    let mut connection = SongMakerConnection {
        endpoint: base.to_string().trim_end_matches('/').to_string(),
        api_version,
        capabilities,
        projects: Vec::new(),
        projects_error: None,
    };

    if !connection
        .capabilities
        .iter()
        .any(|capability| capability == "list_projects")
    {
        connection.projects_error =
            Some("Cet hôte ne propose pas la capacité list_projects.".to_string());
        return Ok(connection);
    }

    let invoke_url = base
        .join("/v1/host/invoke")
        .map_err(|_| "Adresse locale Song Maker invalide.".to_string())?;
    let request_body = serde_json::to_vec(&json!({
        "capability": "list_projects",
        "args": {}
    }))
    .map_err(|_| "Impossible de préparer la demande de projets.".to_string())?;
    let response = client
        .post(invoke_url)
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(request_body)
        .send();

    match response {
        Ok(response) if response.status().is_success() => {
            let body = response
                .bytes()
                .map_err(|_| "La réponse des projets Song Maker est illisible.".to_string())?;
            let payload: Value = serde_json::from_slice(&body).map_err(|_| {
                "La réponse des projets Song Maker n’est pas un JSON valide.".to_string()
            })?;
            if payload.get("ok").and_then(Value::as_bool) != Some(true) {
                connection.projects_error = Some(host_error_message(&payload));
            } else {
                let projects = payload.get("result").unwrap_or(&Value::Null);
                connection.projects = parse_projects(projects);
            }
        }
        Ok(response) => {
            connection.projects_error = Some(format!(
                "La liste des projets a répondu HTTP {}.",
                response.status().as_u16()
            ));
        }
        Err(_) => {
            connection.projects_error =
                Some("Song Maker n’a pas pu charger ses projets.".to_string());
        }
    }

    Ok(connection)
}

fn validate_endpoint(raw: &str) -> Result<Url, String> {
    let value = raw.trim();
    let mut url = Url::parse(value)
        .map_err(|_| "Saisissez l’adresse locale affichée dans Song Maker.".to_string())?;
    if url.scheme() != "http"
        || !url.username().is_empty()
        || url.password().is_some()
        || (url.path() != "/" && !url.path().is_empty())
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("Seule l’adresse HTTP locale de Song Maker est acceptée.".to_string());
    }

    let host = url
        .host_str()
        .ok_or_else(|| "L’adresse Song Maker doit viser localhost.".to_string())?;
    let unbracketed_host = host
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(host);
    let is_loopback = unbracketed_host.eq_ignore_ascii_case("localhost")
        || unbracketed_host
            .parse::<IpAddr>()
            .map(|address| address.is_loopback())
            .unwrap_or(false);
    if !is_loopback {
        return Err("Par sécurité, la connexion Song Maker est limitée à localhost.".to_string());
    }
    url.set_path("");
    Ok(url)
}

fn parse_discovery(body: &Value) -> Result<(String, Vec<String>), String> {
    let api_version = body
        .get("apiVersion")
        .and_then(Value::as_str)
        .ok_or_else(|| "Cette adresse ne renvoie pas un hôte Song Maker.".to_string())?;
    if api_version != API_VERSION {
        return Err(format!("Version de l’hôte incompatible ({api_version})."));
    }

    let music_api = body
        .get("musicApi")
        .ok_or_else(|| "Cette adresse ne fournit pas l’API musique Song Maker.".to_string())?;
    if music_api.get("id").and_then(Value::as_str) != Some(MUSIC_API_ID)
        || music_api.get("kind").and_then(Value::as_str) != Some("music")
        || music_api.get("version").and_then(Value::as_u64) != Some(1)
    {
        return Err(
            "Cette adresse ne fournit pas une API musique Song Maker compatible.".to_string(),
        );
    }

    let capabilities = music_api
        .get("capabilities")
        .and_then(Value::as_array)
        .ok_or_else(|| "L’hôte Song Maker ne déclare aucune capacité.".to_string())?
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    Ok((api_version.to_owned(), capabilities))
}

fn parse_projects(result: &Value) -> Vec<SongMakerProject> {
    let rows = result
        .as_array()
        .or_else(|| result.get("projects").and_then(Value::as_array));
    rows.into_iter()
        .flatten()
        .filter_map(|row| {
            let id = row.get("id")?.as_str()?.to_owned();
            let title = row.get("title")?.as_str()?.to_owned();
            Some(SongMakerProject {
                id,
                title,
                updated_at: row
                    .get("updatedAt")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            })
        })
        .collect()
}

fn host_error_message(payload: &Value) -> String {
    payload
        .get("messageFr")
        .and_then(Value::as_str)
        .filter(|message| !message.trim().is_empty())
        .unwrap_or("Song Maker a refusé la demande de projets.")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn endpoint_validation_only_accepts_plain_http_loopback() {
        assert!(validate_endpoint("http://127.0.0.1:43125").is_ok());
        assert!(validate_endpoint("http://[::1]:43125/").is_ok());
        assert!(validate_endpoint("http://song-maker.local:43125").is_err());
        assert!(validate_endpoint("https://127.0.0.1:43125").is_err());
        assert!(validate_endpoint("http://127.0.0.1:43125/proxy").is_err());
    }

    #[test]
    fn discovery_accepts_the_embedded_library_only_host() {
        let response = json!({
            "apiVersion": "song-maker.host.v1",
            "hostId": "akasha",
            "musicApi": {
                "id": "song-maker-music",
                "kind": "music",
                "version": 1,
                "capabilities": ["list_projects"]
            }
        });
        let (version, capabilities) = parse_discovery(&response).expect("valid discovery");
        assert_eq!(version, API_VERSION);
        assert_eq!(capabilities, ["list_projects"]);
    }

    #[test]
    fn discovery_rejects_tts_or_unrelated_hosts() {
        let response = json!({
            "apiVersion": "song-maker.host.v1",
            "musicApi": {
                "id": "speech",
                "kind": "tts",
                "version": 1,
                "capabilities": ["list_projects"]
            }
        });
        assert!(parse_discovery(&response).is_err());
    }

    #[test]
    fn configured_endpoint_wins_when_another_loopback_host_is_available() {
        let selected = TcpListener::bind("127.0.0.1:0").expect("bind selected host");
        let selected_address = selected.local_addr().expect("selected address");
        let other = TcpListener::bind("127.0.0.1:0").expect("bind other host");
        other.set_nonblocking(true).expect("nonblocking other host");

        let server = thread::spawn(move || {
            for (expected_path, response_body) in [
                (
                    "/v1/host/discover",
                    r#"{"apiVersion":"song-maker.host.v1","hostId":"selected-song-maker","musicApi":{"id":"song-maker-music","kind":"music","version":1,"capabilities":["list_projects"]}}"#,
                ),
                (
                    "/v1/host/invoke",
                    r#"{"ok":true,"result":[{"id":"selected-project","title":"Configured endpoint project"}]}"#,
                ),
            ] {
                let (mut stream, _) = selected.accept().expect("selected endpoint request");
                let mut reader = BufReader::new(stream.try_clone().expect("clone socket"));
                let mut first_line = String::new();
                reader.read_line(&mut first_line).expect("request line");
                assert!(first_line.contains(expected_path));

                let mut content_length = 0usize;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).expect("header line");
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        content_length = value.trim().parse().unwrap_or(0);
                    }
                }
                let mut request_body = vec![0; content_length];
                reader.read_exact(&mut request_body).expect("request body");

                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    response_body.len(),
                    response_body
                );
                stream
                    .write_all(response.as_bytes())
                    .expect("write response");
            }
        });

        let connection = connect_and_list(&format!("http://{selected_address}"))
            .expect("use the configured host");
        server.join().expect("selected host server");
        assert_eq!(connection.endpoint, format!("http://{selected_address}"));
        assert_eq!(connection.projects.len(), 1);
        assert_eq!(connection.projects[0].id, "selected-project");
        assert_eq!(connection.projects[0].title, "Configured endpoint project");
        assert!(matches!(
            other.accept(),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock
        ));
    }

    #[test]
    fn native_client_discovers_and_invokes_project_listing() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local mock");
        let address = listener.local_addr().expect("mock address");
        let server = thread::spawn(move || {
            for (expected_path, response_body) in [
                (
                    "/v1/host/discover",
                    r#"{"apiVersion":"song-maker.host.v1","hostId":"akasha","musicApi":{"id":"song-maker-music","kind":"music","version":1,"capabilities":["list_projects"]}}"#,
                ),
                (
                    "/v1/host/invoke",
                    r#"{"ok":true,"result":[{"id":"project-1","title":"Ma chanson","updatedAt":"2026-10-11"}]}"#,
                ),
            ] {
                let (mut stream, _) = listener.accept().expect("client request");
                let mut reader = BufReader::new(stream.try_clone().expect("clone socket"));
                let mut first_line = String::new();
                reader.read_line(&mut first_line).expect("request line");
                assert!(first_line.contains(expected_path));

                let mut content_length = 0usize;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).expect("header line");
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        content_length = value.trim().parse().unwrap_or(0);
                    }
                }
                let mut request_body = vec![0; content_length];
                reader.read_exact(&mut request_body).expect("request body");
                if expected_path.ends_with("invoke") {
                    let body: Value = serde_json::from_slice(&request_body).expect("invoke JSON");
                    assert_eq!(body["capability"], "list_projects");
                    assert_eq!(body["args"], json!({}));
                }

                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    response_body.len(),
                    response_body
                );
                stream
                    .write_all(response.as_bytes())
                    .expect("write response");
            }
        });

        let connection = connect_and_list(&format!("http://{address}"))
            .expect("discover Song Maker and list projects");
        server.join().expect("mock server");
        assert_eq!(connection.api_version, API_VERSION);
        assert_eq!(connection.capabilities, ["list_projects"]);
        assert_eq!(connection.projects.len(), 1);
        assert_eq!(connection.projects[0].title, "Ma chanson");
        assert_eq!(
            connection.projects[0].updated_at.as_deref(),
            Some("2026-10-11")
        );
        assert!(connection.projects_error.is_none());
    }
}
