//! Recherche web + téléchargement HTTP + browse HTML→texte (Preview PC.8–PC.9).
//!
//! Search : Brave API (clé) / SearXNG JSON / DuckDuckGo HTML / Bing HTML, chaîne `auto`.

use aos_proto::{WebBrowseResponse, WebSearchHit, WebSearchResponse};
use crate::net::EgressControl;
use base64::Engine as _;
use std::collections::HashSet;
use std::io::Read;

/// Desktop Chrome UA — the previous AgentOS UA triggered DDG's anomaly wall.
const SEARCH_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36";

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

/// Recherche web multi-moteurs.
/// `engine` : `auto` | `brave` | `searxng` | `duckduckgo` | `bing`.
pub fn web_search(
    net: &mut EgressControl,
    actor: &str,
    caps: &[String],
    query: &str,
    max_results: usize,
    brave_key: Option<&str>,
    engine: &str,
) -> Result<WebSearchResponse, NetSvcError> {
    let engine = engine.trim().to_ascii_lowercase();
    let engine = if engine.is_empty() {
        "auto"
    } else {
        engine.as_str()
    };
    let searxng = searxng_url_from_prefs();

    match engine {
        "brave" => {
            let key = brave_key.filter(|k| !k.is_empty()).ok_or_else(|| {
                NetSvcError::Unsupported(
                    "clé Brave absente (secret brave_search_api_key)".into(),
                )
            })?;
            brave_search(net, actor, caps, query, max_results, key)
        }
        "searxng" | "searx" => {
            let url = searxng.as_deref().ok_or_else(|| {
                NetSvcError::Unsupported(
                    "SearXNG: URL d'instance absente (Settings → outils web)".into(),
                )
            })?;
            searxng_search(net, actor, caps, query, max_results, url)
        }
        "duckduckgo" | "ddg" => ddg_search(net, actor, caps, query, max_results),
        "bing" => bing_search(net, actor, caps, query, max_results),
        _ => {
            // auto : Brave → SearXNG → DDG → Bing. Empty HTML scrapes are errors.
            let mut errors: Vec<String> = Vec::new();
            if let Some(key) = brave_key.filter(|k| !k.is_empty()) {
                match brave_search(net, actor, caps, query, max_results, key) {
                    Ok(r) if !r.results.is_empty() => return Ok(r),
                    Ok(_) => errors.push("brave: 0 résultat".into()),
                    Err(e) => errors.push(format!("brave: {e}")),
                }
            }
            if let Some(url) = searxng.as_deref() {
                match searxng_search(net, actor, caps, query, max_results, url) {
                    Ok(r) if !r.results.is_empty() => return Ok(r),
                    Ok(_) => errors.push("searxng: 0 résultat".into()),
                    Err(e) => errors.push(format!("searxng: {e}")),
                }
            }
            match ddg_search(net, actor, caps, query, max_results) {
                Ok(r) if !r.results.is_empty() => return Ok(r),
                Ok(_) => errors.push("duckduckgo: 0 résultat".into()),
                Err(e) => errors.push(format!("duckduckgo: {e}")),
            }
            match bing_search(net, actor, caps, query, max_results) {
                Ok(r) if !r.results.is_empty() => return Ok(r),
                Ok(_) => errors.push("bing: 0 résultat".into()),
                Err(e) => errors.push(format!("bing: {e}")),
            }
            Err(NetSvcError::Http(if errors.is_empty() {
                "aucun moteur disponible".into()
            } else {
                errors.join(" → ")
            }))
        }
    }
}

fn searxng_url_from_prefs() -> Option<String> {
    let home = std::env::var("AOS_HOME").unwrap_or_else(|_| ".".into());
    let raw = std::fs::read_to_string(
        std::path::Path::new(&home).join("var/run/preferences.json"),
    )
    .ok()?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let s = v.get("searxng_url")?.as_str()?.trim();
    if s.starts_with("http://") || s.starts_with("https://") {
        Some(s.trim_end_matches('/').to_string())
    } else {
        None
    }
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

fn searxng_search(
    net: &mut EgressControl,
    actor: &str,
    caps: &[String],
    query: &str,
    max_results: usize,
    instance: &str,
) -> Result<WebSearchResponse, NetSvcError> {
    let base = instance.trim().trim_end_matches('/');
    let (host, port) = parse_host_port(base)?;
    net.grant(format!("net.connect:{host}:{port}"));
    ensure_egress(net, actor, &host, port, caps)?;
    let url = format!(
        "{base}/search?q={}&format=json&categories=general",
        urlencoding(query)
    );
    let body = http_get_text(net, actor, caps, &url, Some("application/json"))?;
    parse_searxng_json(&body, max_results)
}

fn parse_searxng_json(body: &str, max_results: usize) -> Result<WebSearchResponse, NetSvcError> {
    let v: serde_json::Value = serde_json::from_str(body).map_err(|_| {
        NetSvcError::Http("searxng: réponse non-JSON (format=json souvent désactivé)".into())
    })?;
    let mut results = Vec::new();
    if let Some(arr) = v.get("results").and_then(|x| x.as_array()) {
        for item in arr.iter().take(max_results.max(1)) {
            let url = item.get("url").and_then(|t| t.as_str()).unwrap_or("");
            let title = item.get("title").and_then(|t| t.as_str()).unwrap_or("");
            if url.starts_with("http") && !title.is_empty() {
                results.push(WebSearchHit {
                    title: title.into(),
                    url: url.into(),
                    snippet: item
                        .get("content")
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .into(),
                });
            }
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
    let html = http_get_text(net, actor, caps, &url, None)?;
    if ddg_html_blocked(&html) {
        return Err(NetSvcError::Http("duckduckgo: challenge anti-bot".into()));
    }
    let results = parse_ddg_html(&html, max_results);
    if results.is_empty() {
        return Err(NetSvcError::Http(
            "duckduckgo: 0 résultat (HTML inattendu)".into(),
        ));
    }
    Ok(WebSearchResponse { results })
}

fn ddg_html_blocked(html: &str) -> bool {
    html.contains("anomaly-modal") || html.contains("anomaly.js")
}

fn bing_search(
    net: &mut EgressControl,
    actor: &str,
    caps: &[String],
    query: &str,
    max_results: usize,
) -> Result<WebSearchResponse, NetSvcError> {
    ensure_egress(net, actor, "www.bing.com", 443, caps)?;
    let url = format!("https://www.bing.com/search?q={}", urlencoding(query));
    let html = http_get_text(net, actor, caps, &url, None)?;
    let results = parse_bing_html(&html, max_results);
    if results.is_empty() {
        return Err(NetSvcError::Http("bing: 0 résultat (HTML inattendu)".into()));
    }
    Ok(WebSearchResponse { results })
}

fn http_get_text(
    net: &mut EgressControl,
    actor: &str,
    caps: &[String],
    url: &str,
    accept: Option<&str>,
) -> Result<String, NetSvcError> {
    let (host, port) = parse_host_port(url)?;
    ensure_egress(net, actor, &host, port, caps)?;
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .user_agent(SEARCH_UA)
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|e| NetSvcError::Http(e.to_string()))?;
    let mut req = client
        .get(url)
        .header("Accept-Language", "en-US,en;q=0.9,fr;q=0.8");
    if let Some(a) = accept {
        req = req.header("Accept", a);
    }
    let resp = req.send().map_err(|e| NetSvcError::Http(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(NetSvcError::Http(format!("status {}", resp.status())));
    }
    resp.text()
        .map_err(|e| NetSvcError::Http(e.to_string()))
}

fn parse_ddg_html(html: &str, max_results: usize) -> Vec<WebSearchHit> {
    let mut results = Vec::new();
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

fn parse_bing_html(html: &str, max_results: usize) -> Vec<WebSearchHit> {
    let mut results = Vec::new();
    let mut seen = HashSet::new();
    let mut rest = html;
    while results.len() < max_results {
        // Liens résultats : <h2><a href="https://...">title</a></h2> dans li.b_algo
        let Some(algo) = rest.find("b_algo") else {
            break;
        };
        rest = &rest[algo + 6..];
        let Some(h2) = rest.find("<h2") else {
            continue;
        };
        let after_h2 = &rest[h2..];
        let Some(href_pos) = after_h2.find("href=\"") else {
            continue;
        };
        let after = &after_h2[href_pos + 6..];
        let Some(end) = after.find('"') else {
            break;
        };
        let url = decode_bing_href(&after[..end]);
        let title = extract_between(&after[end..], ">", "</a>")
            .map(|t| strip_tags(&t))
            .unwrap_or_default();
        if title.is_empty() || is_search_noise_url(&url) || !seen.insert(url.clone()) {
            continue;
        }
        let snippet = rest
            .find("b_caption")
            .or_else(|| rest.find("b_lineclamp"))
            .and_then(|i| {
                let chunk = &rest[i..i.saturating_add(800).min(rest.len())];
                extract_between(chunk, ">", "</p>")
                    .or_else(|| extract_between(chunk, ">", "</div>"))
            })
            .map(|s| strip_tags(&s))
            .unwrap_or_default();
        results.push(WebSearchHit {
            title,
            url,
            snippet,
        });
    }
    results
}

fn decode_bing_href(raw: &str) -> String {
    let url = html_unescape(raw);
    if let Some(dest) = bing_ck_destination(&url) {
        return dest;
    }
    url
}

/// Bing wraps results in `/ck/a?...&u=a1<base64-url>`.
fn bing_ck_destination(url: &str) -> Option<String> {
    let rest = url
        .split("&u=")
        .nth(1)
        .or_else(|| url.split("?u=").nth(1))?;
    let payload = rest.split('&').next()?.trim();
    let b64 = payload.strip_prefix("a1").unwrap_or(payload);
    base64_decode_to_string(b64).filter(|s| s.starts_with("http"))
}

fn base64_decode_to_string(s: &str) -> Option<String> {
    let mut padded = s.replace('-', "+").replace('_', "/");
    while padded.len() % 4 != 0 {
        padded.push('=');
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(padded.as_bytes())
        .ok()?;
    String::from_utf8(bytes).ok()
}

fn is_search_noise_url(url: &str) -> bool {
    let u = url.to_ascii_lowercase();
    !(u.starts_with("http://") || u.starts_with("https://"))
        || u.contains("bing.com")
        || u.contains("microsoft.com")
        || u.contains("msn.com")
}

/// Lit une page HTML et renvoie un texte utilisable par le LLM (sans JS).
pub fn web_browse(
    net: &mut EgressControl,
    actor: &str,
    caps: &[String],
    url: &str,
    max_chars: usize,
) -> Result<WebBrowseResponse, NetSvcError> {
    let parsed = url::Url::parse(url).map_err(|e| NetSvcError::BadUrl(e.to_string()))?;
    match parsed.scheme() {
        "http" | "https" => {}
        other => {
            return Err(NetSvcError::Unsupported(format!(
                "schéma non supporté: {other}"
            )))
        }
    }
    let max_bytes = 2_000_000u64;
    let (bytes, ctype) = http_fetch_bytes(net, actor, caps, url, max_bytes)?;
    let raw = String::from_utf8_lossy(&bytes);
    if !ctype.contains("html")
        && !ctype.contains("text/")
        && !raw.trim_start().starts_with('<')
    {
        // Texte brut non-HTML
        let text: String = raw.chars().take(max_chars.max(1)).collect();
        return Ok(WebBrowseResponse {
            url: url.to_string(),
            final_url: url.to_string(),
            title: String::new(),
            text,
            bytes: bytes.len() as u64,
        });
    }
    let (title, text) = html_to_readable(&raw, max_chars.max(1));
    Ok(WebBrowseResponse {
        url: url.to_string(),
        final_url: url.to_string(),
        title,
        text,
        bytes: bytes.len() as u64,
    })
}

fn html_to_readable(html: &str, max_chars: usize) -> (String, String) {
    let title = extract_between(html, "<title", "</title>")
        .or_else(|| extract_between(html, "<TITLE", "</TITLE>"))
        .map(|t| {
            let t = t.find('>').map(|i| &t[i + 1..]).unwrap_or(&t);
            strip_tags(t)
        })
        .unwrap_or_default();

    // Retire script / style / noscript
    let mut cleaned = html.to_string();
    for tag in ["script", "style", "noscript", "svg", "iframe"] {
        cleaned = strip_tag_blocks(&cleaned, tag);
    }
    // Préfère <main> / <article> si présents
    let body_src = extract_between(&cleaned, "<main", "</main>")
        .or_else(|| extract_between(&cleaned, "<article", "</article>"))
        .or_else(|| extract_between(&cleaned, "<body", "</body>"))
        .unwrap_or(cleaned);
    let body_src = body_src
        .find('>')
        .map(|i| body_src[i + 1..].to_string())
        .unwrap_or(body_src);

    let mut text = strip_tags(&body_src);
    // Collapse whitespace
    let mut out = String::new();
    let mut prev_space = false;
    for c in text.chars() {
        if c.is_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(c);
            prev_space = false;
        }
    }
    text = out.trim().to_string();
    if text.chars().count() > max_chars {
        text = text.chars().take(max_chars).collect::<String>() + "…";
    }
    (title, text)
}

fn strip_tag_blocks(html: &str, tag: &str) -> String {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let open_l = format!("<{}", tag.to_ascii_uppercase());
    let close_l = format!("</{}>", tag.to_ascii_uppercase());
    let mut s = html.to_string();
    for (o, c) in [(&open[..], &close[..]), (&open_l[..], &close_l[..])] {
        while let Some(start) = s.to_ascii_lowercase().find(&o.to_ascii_lowercase()) {
            let after = start + o.len();
            if let Some(rel) = s[after..].to_ascii_lowercase().find(&c.to_ascii_lowercase()) {
                let end = after + rel + c.len();
                s = format!("{}{}", &s[..start], &s[end..]);
            } else {
                s.truncate(start);
                break;
            }
        }
    }
    s
}

fn extract_between(s: &str, start: &str, end: &str) -> Option<String> {
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
        .replace("&nbsp;", " ")
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
        .redirect(reqwest::redirect::Policy::limited(5))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bing_ck_decodes_a1_base64() {
        let href = "https://www.bing.com/ck/a?!&amp;&amp;p=abc&amp;u=a1aHR0cHM6Ly9lbi53aWtpcGVkaWEub3JnL3dpa2kvRGV2aW5fQUk&amp;ntb=1";
        assert_eq!(decode_bing_href(href), "https://en.wikipedia.org/wiki/Devin_AI");
    }

    #[test]
    fn parse_bing_html_unwraps_ck_tracking_links() {
        let html = r#"<ol id="b_results"><li class="b_algo"><div class="b_tpcn"></div>
<h2><a target="_blank" href="https://www.bing.com/ck/a?!&amp;&amp;p=x&amp;u=a1aHR0cHM6Ly9lbi53aWtpcGVkaWEub3JnL3dpa2kvRGV2aW5fQUk&amp;ntb=1">Devin AI - Wikipedia</a></h2>
<div class="b_caption"><p class="b_lineclamp2">Devin is an autonomous AI software engineer.</p></div></li>
<li class="b_algo"><h2><a href="https://www.bing.com/ck/a?!&amp;u=a1aHR0cHM6Ly93d3cuZGV2aW4uYWkv">Devin</a></h2>
<div class="b_caption"><p>The official site.</p></div></li></ol>"#;
        let hits = parse_bing_html(html, 5);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].url, "https://en.wikipedia.org/wiki/Devin_AI");
        assert_eq!(hits[0].title, "Devin AI - Wikipedia");
        assert!(hits[0].snippet.contains("autonomous"));
        assert_eq!(hits[1].url, "https://www.devin.ai/");
    }

    #[test]
    fn parse_bing_html_drops_undecoded_tracking() {
        let html = r#"<li class="b_algo"><h2><a href="https://www.bing.com/ck/a?!&amp;p=nope">No dest</a></h2></li>"#;
        assert!(parse_bing_html(html, 5).is_empty());
    }

    #[test]
    fn ddg_anomaly_page_is_blocked() {
        assert!(ddg_html_blocked(
            r#"<div class="anomaly-modal__mask"></div><script src="anomaly.js"></script>"#
        ));
        assert!(!ddg_html_blocked(
            r#"<a class="result__a" href="https://example.com">Example</a>"#
        ));
    }

    #[test]
    fn parse_searxng_json_hits() {
        let body = r#"{"results":[{"title":"Devin","url":"https://devin.ai","content":"AI engineer"},{"title":"","url":"https://skip.me"}]}"#;
        let resp = parse_searxng_json(body, 5).unwrap();
        assert_eq!(resp.results.len(), 1);
        assert_eq!(resp.results[0].url, "https://devin.ai");
        assert_eq!(resp.results[0].snippet, "AI engineer");
    }

    #[test]
    fn parse_searxng_rejects_html() {
        assert!(parse_searxng_json("<html>nope</html>", 5).is_err());
    }
}
