//! Recherche web + téléchargement HTTP (Preview PC.8–PC.9).
//!
//! Backend search v1 : DuckDuckGo HTML (sans clé). Optionnel : Brave Search
//! si secret `brave_search_api_key` présent.

use aos_proto::{WebSearchHit, WebSearchResponse};
use crate::net::EgressControl;
use std::io::Read;

#[derive(Debug)]
pub enum NetSvcError {
    Offline,
    Denied(String),
    Http(String),
    TooLarge(u64),
    BadUrl(String),
    Unsupported(String),
}

impl std::fmt::Display for NetSvcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Offline => write!(f, "réseau désactivé (offline_strict)"),
            Self::Denied(s) => write!(f, "egress refusé: {s}"),
            Self::Http(s) => write!(f, "http: {s}"),
            Self::TooLarge(n) => write!(f, "fichier trop volumineux (>{n} octets)"),
            Self::BadUrl(s) => write!(f, "url invalide: {s}"),
            Self::Unsupported(s) => write!(f, "{s}"),
        }
    }
}

pub fn parse_host_port(url: &str) -> Result<(String, u16), NetSvcError> {
    let u = url::Url::parse(url).map_err(|e| NetSvcError::BadUrl(e.to_string()))?;
    let host = u
        .host_str()
        .ok_or_else(|| NetSvcError::BadUrl("hôte manquant".into()))?
        .to_string();
    let port = u.port_or_known_default().unwrap_or(443);
    Ok((host, port))
}

fn ensure_egress(
    net: &mut EgressControl,
    actor: &str,
    host: &str,
    port: u16,
    caps: &[String],
) -> Result<(), NetSvcError> {
    if !net.check(actor, host, port, caps) {
        if matches!(net.mode(), crate::net::NetMode::OfflineStrict) {
            return Err(NetSvcError::Offline);
        }
        return Err(NetSvcError::Denied(format!("{host}:{port}")));
    }
    Ok(())
}

/// Recherche web (DDG HTML lite).
pub fn web_search(
    net: &mut EgressControl,
    actor: &str,
    caps: &[String],
    query: &str,
    max_results: usize,
    brave_key: Option<&str>,
) -> Result<WebSearchResponse, NetSvcError> {
    if let Some(key) = brave_key.filter(|k| !k.is_empty()) {
        return brave_search(net, actor, caps, query, max_results, key);
    }
    ddg_search(net, actor, caps, query, max_results)
}

fn brave_search(
    net: &mut EgressControl,
    actor: &str,
    caps: &[String],
    query: &str,
    max_results: usize,
    api_key: &str,
) -> Result<WebSearchResponse, NetSvcError> {
    let host = "api.search.brave.com";
    ensure_egress(net, actor, host, 443, caps)?;
    let url = format!(
        "https://api.search.brave.com/res/v1/web/search?q={}&count={}",
        urlencoding(query),
        max_results.clamp(1, 20)
    );
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| NetSvcError::Http(e.to_string()))?;
    let resp = client
        .get(&url)
        .header("Accept", "application/json")
        .header("X-Subscription-Token", api_key)
        .send()
        .map_err(|e| NetSvcError::Http(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(NetSvcError::Http(format!("status {}", resp.status())));
    }
    let v: serde_json::Value = resp.json().map_err(|e| NetSvcError::Http(e.to_string()))?;
    let mut results = Vec::new();
    if let Some(arr) = v.pointer("/web/results").and_then(|x| x.as_array()) {
        for item in arr.iter().take(max_results) {
            results.push(WebSearchHit {
                title: item
                    .get("title")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .into(),
                url: item
                    .get("url")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .into(),
                snippet: item
                    .get("description")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .into(),
            });
        }
    }
    Ok(WebSearchResponse { results })
}

fn ddg_search(
    net: &mut EgressControl,
    actor: &str,
    caps: &[String],
    query: &str,
    max_results: usize,
) -> Result<WebSearchResponse, NetSvcError> {
    ensure_egress(net, actor, "html.duckduckgo.com", 443, caps)?;
    let url = format!(
        "https://html.duckduckgo.com/html/?q={}",
        urlencoding(query)
    );
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .user_agent("AgentOS-Preview/0.1")
        .build()
        .map_err(|e| NetSvcError::Http(e.to_string()))?;
    let html = client
        .get(&url)
        .send()
        .map_err(|e| NetSvcError::Http(e.to_string()))?
        .text()
        .map_err(|e| NetSvcError::Http(e.to_string()))?;
    Ok(WebSearchResponse {
        results: parse_ddg_html(&html, max_results),
    })
}

fn parse_ddg_html(html: &str, max_results: usize) -> Vec<WebSearchHit> {
    let mut results = Vec::new();
    // Résultats : <a class="result__a" href="...">title</a> + snippet result__snippet
    let mut rest = html;
    while results.len() < max_results {
        let Some(idx) = rest.find("result__a") else {
            break;
        };
        rest = &rest[idx..];
        let Some(href_pos) = rest.find("href=\"") else {
            break;
        };
        let after = &rest[href_pos + 6..];
        let Some(end) = after.find('"') else {
            break;
        };
        let mut url = after[..end].to_string();
        // DDG wrap : //duckduckgo.com/l/?uddg=<encoded>
        if let Some(q) = url.find("uddg=") {
            let enc = &url[q + 5..];
            let enc = enc.split('&').next().unwrap_or(enc);
            if let Ok(decoded) = urlencoding_decode(enc) {
                url = decoded;
            }
        }
        let after_a = after.get(end..).unwrap_or("");
        let title = extract_between(after_a, ">", "</a>").unwrap_or_default();
        let snippet = rest
            .find("result__snippet")
            .and_then(|i| extract_between(&rest[i..], ">", "</"))
            .unwrap_or_default();
        let title = strip_tags(&title);
        let snippet = strip_tags(&snippet);
        if !title.is_empty() && !url.is_empty() {
            results.push(WebSearchHit {
                title,
                url,
                snippet,
            });
        }
        rest = rest.get(10..).unwrap_or("");
    }
    results
}

fn extract_between<'a>(s: &'a str, start: &str, end: &str) -> Option<String> {
    let i = s.find(start)? + start.len();
    let rest = &s[i..];
    let j = rest.find(end)?;
    Some(rest[..j].to_string())
}

fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    html_unescape(out.trim())
}

fn html_unescape(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
}

fn urlencoding(s: &str) -> String {
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

fn urlencoding_decode(s: &str) -> Result<String, ()> {
    let bytes = url::form_urlencoded::parse(s.replace('+', " ").as_bytes())
        .map(|(k, _)| k.into_owned())
        .next();
    // uddg is a single encoded URL string
    let mut out = Vec::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            let h1 = chars.next().ok_or(())?;
            let h2 = chars.next().ok_or(())?;
            let hex = format!("{h1}{h2}");
            let b = u8::from_str_radix(&hex, 16).map_err(|_| ())?;
            out.push(b);
        } else if c == '+' {
            out.push(b' ');
        } else {
            out.push(c as u8);
        }
    }
    let _ = bytes;
    String::from_utf8(out).map_err(|_| ())
}

/// Télécharge une URL vers un buffer (plafond `max_bytes`).
pub fn http_fetch_bytes(
    net: &mut EgressControl,
    actor: &str,
    caps: &[String],
    url: &str,
    max_bytes: u64,
) -> Result<(Vec<u8>, String), NetSvcError> {
    let (host, port) = parse_host_port(url)?;
    ensure_egress(net, actor, &host, port, caps)?;
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .user_agent("AgentOS-Preview/0.1")
        .build()
        .map_err(|e| NetSvcError::Http(e.to_string()))?;
    let mut resp = client
        .get(url)
        .send()
        .map_err(|e| NetSvcError::Http(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(NetSvcError::Http(format!("status {}", resp.status())));
    }
    let ctype = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();
    if let Some(len) = resp.content_length() {
        if len > max_bytes {
            return Err(NetSvcError::TooLarge(max_bytes));
        }
    }
    let mut buf = Vec::new();
    let mut limited = resp.by_ref().take(max_bytes + 1);
    limited
        .read_to_end(&mut buf)
        .map_err(|e| NetSvcError::Http(e.to_string()))?;
    if buf.len() as u64 > max_bytes {
        return Err(NetSvcError::TooLarge(max_bytes));
    }
    Ok((buf, ctype))
}

pub fn safe_download_name(url: &str) -> String {
    let raw = url::Url::parse(url)
        .ok()
        .and_then(|u| {
            u.path_segments()
                .and_then(|mut s| s.next_back().map(|x| x.to_string()))
        })
        .filter(|s| !s.is_empty() && s.contains('.'))
        .unwrap_or_else(|| {
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            format!("file-{ts}.bin")
        });
    raw.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}
