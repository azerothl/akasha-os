//! Recherche web + téléchargement HTTP + browse HTML→texte (Preview PC.8–PC.9).
//!
//! Search : Brave API (clé) / DuckDuckGo HTML / Bing HTML, avec chaîne `auto`.

use aos_proto::{WebBrowseResponse, WebSearchHit, WebSearchResponse};
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

/// Recherche web multi-moteurs.
/// `engine` : `auto` | `brave` | `duckduckgo` | `bing`.
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

    match engine {
        "brave" => {
            let key = brave_key.filter(|k| !k.is_empty()).ok_or_else(|| {
                NetSvcError::Unsupported(
                    "clé Brave absente (secret brave_search_api_key)".into(),
                )
            })?;
            brave_search(net, actor, caps, query, max_results, key)
        }
        "duckduckgo" | "ddg" => ddg_search(net, actor, caps, query, max_results),
        "bing" => bing_search(net, actor, caps, query, max_results),
        _ => {
            // auto : Brave → DDG → Bing
            let mut last_err: Option<NetSvcError> = None;
            if let Some(key) = brave_key.filter(|k| !k.is_empty()) {
                match brave_search(net, actor, caps, query, max_results, key) {
                    Ok(r) if !r.results.is_empty() => return Ok(r),
                    Ok(_) => {}
                    Err(e) => last_err = Some(e),
                }
            }
            match ddg_search(net, actor, caps, query, max_results) {
                Ok(r) if !r.results.is_empty() => return Ok(r),
                Ok(_) => {}
                Err(e) => last_err = Some(e),
            }
            match bing_search(net, actor, caps, query, max_results) {
                Ok(r) => Ok(r),
                Err(e) => Err(last_err.unwrap_or(e)),
            }
        }
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
    let html = http_get_text(net, actor, caps, &url)?;
    Ok(WebSearchResponse {
        results: parse_ddg_html(&html, max_results),
    })
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
    let html = http_get_text(net, actor, caps, &url)?;
    Ok(WebSearchResponse {
        results: parse_bing_html(&html, max_results),
    })
}

fn http_get_text(
    net: &mut EgressControl,
    actor: &str,
    caps: &[String],
    url: &str,
) -> Result<String, NetSvcError> {
    let (host, port) = parse_host_port(url)?;
    ensure_egress(net, actor, &host, port, caps)?;
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .user_agent("AgentOS-Preview/0.1 (compatible; +https://github.com/azerothl/akasha-os)")
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|e| NetSvcError::Http(e.to_string()))?;
    let resp = client
        .get(url)
        .send()
        .map_err(|e| NetSvcError::Http(e.to_string()))?;
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
    let mut rest = html;
    while results.len() < max_results {
        // Liens résultats : <h2><a href="https://...">title</a></h2> dans li.b_algo
        let Some(algo) = rest.find("b_algo") else {
            break;
        };
        rest = &rest[algo..];
        let Some(h2) = rest.find("<h2") else {
            rest = rest.get(6..).unwrap_or("");
            continue;
        };
        let after_h2 = &rest[h2..];
        let Some(href_pos) = after_h2.find("href=\"") else {
            rest = rest.get(6..).unwrap_or("");
            continue;
        };
        let after = &after_h2[href_pos + 6..];
        let Some(end) = after.find('"') else {
            break;
        };
        let url = after[..end].to_string();
        let title = extract_between(&after[end..], ">", "</a>")
            .map(|t| strip_tags(&t))
            .unwrap_or_default();
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
        if !title.is_empty()
            && url.starts_with("http")
            && !url.contains("bing.com/ck/")
            && !url.contains("microsoft.com")
        {
            results.push(WebSearchHit {
                title,
                url,
                snippet,
            });
        }
        rest = rest.get(6..).unwrap_or("");
    }
    // Fallback plus large si parse b_algo vide
    if results.is_empty() {
        let mut rest = html;
        while results.len() < max_results {
            let Some(href_pos) = rest.find("href=\"http") else {
                break;
            };
            let after = &rest[href_pos + 6..];
            let Some(end) = after.find('"') else {
                break;
            };
            let url = after[..end].to_string();
            let title = extract_between(&after[end..], ">", "</a>")
                .map(|t| strip_tags(&t))
                .unwrap_or_default();
            rest = &after[end..];
            if title.len() > 8
                && url.starts_with("http")
                && !url.contains("bing.com")
                && !url.contains("microsoft.com")
                && !url.contains("msn.com")
            {
                results.push(WebSearchHit {
                    title,
                    url,
                    snippet: String::new(),
                });
            }
        }
    }
    results
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
