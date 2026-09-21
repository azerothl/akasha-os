//! Backend distant OpenAI-compatible (P3.1, §3.3) : client HTTP/SSE,
//! clé API via le service de secrets (jamais exposée aux agents, §9.2).

use aos_proto::{InferRequest, TokenEvent};
use futures::StreamExt;
use base64::Engine;
use std::io::Read;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RemoteError {
    #[error("http: {0}")]
    Http(String),
    #[error("flux SSE: {0}")]
    Sse(String),
    #[error("image: {0}")]
    Image(String),
}

#[cfg(test)]
mod vision_tests {
    use super::*;

    fn request() -> InferRequest {
        InferRequest {
            model_id: None,
            messages: vec![
                aos_proto::ChatMessage { role: "user".into(), content: "earlier".into() },
                aos_proto::ChatMessage { role: "assistant".into(), content: "reply".into() },
                aos_proto::ChatMessage { role: "user".into(), content: "compare".into() },
            ],
            tools: vec![], params: Default::default(), priority: 1,
            data_refs: vec![], images: vec![], routing: None,
        }
    }

    #[test]
    fn text_request_is_unchanged() {
        let backend = RemoteOpenAiBackend::new("http://127.0.0.1:11434/v1", "test", None);
        let body = backend.request_body(&request()).unwrap();
        assert_eq!(body["messages"][2]["content"], "compare");
    }

    #[test]
    fn transient_ollama_is_explicit_local_and_releases_after_request() {
        let mut backend = RemoteOpenAiBackend::new("http://127.0.0.1:11434/v1", "test", None);
        assert!(!backend.release_ollama_after_response);
        backend.enable_transient_ollama().unwrap();
        let body = backend.ollama_request_body(&request()).unwrap();
        assert_eq!(body["keep_alive"], 0);
        assert_eq!(body["messages"][2]["content"], "compare");
        assert_eq!(body["options"]["num_ctx"], 9216);
        for url in ["https://example.com/v1", "http://127.0.0.1:11434/other"] {
            assert!(RemoteOpenAiBackend::new(url, "test", None).enable_transient_ollama().is_err());
        }
    }

    #[test]
    fn image_pair_attaches_bytes_in_order_to_last_user_only() {
        let root = std::env::temp_dir().join(format!("aos-vision-{}-{}", std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir(&root).unwrap();
        let mut req = request();
        let mut originals = Vec::new();
        for (name, color) in [("source.png", [255u8, 0, 0]), ("candidate.png", [0, 255, 0])] {
            let path = root.join(name);
            image::RgbImage::from_pixel(2, 2, image::Rgb(color)).save(&path).unwrap();
            originals.push(std::fs::read(&path).unwrap());
            req.images.push(path.to_string_lossy().into_owned());
        }
        let body = RemoteOpenAiBackend::new("http://127.0.0.1:11434/v1", "test", None)
            .request_body(&req).unwrap();
        let native = RemoteOpenAiBackend::new("http://127.0.0.1:11434/v1", "test", None)
            .ollama_request_body(&req).unwrap();
        let native_images = native["messages"][2]["images"].as_array().unwrap();
        assert_eq!(native_images.len(), 2);
        for (encoded, expected) in native_images.iter().zip(&originals) {
            assert_eq!(&base64::engine::general_purpose::STANDARD.decode(encoded.as_str().unwrap()).unwrap(), expected);
        }
        assert_eq!(body["messages"][0]["content"], "earlier");
        assert_eq!(body["messages"][1]["content"], "reply");
        let parts = body["messages"][2]["content"].as_array().unwrap();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0]["text"], "compare");
        for (part, expected) in parts[1..].iter().zip(originals) {
            let uri = part["image_url"]["url"].as_str().unwrap();
            let bytes = base64::engine::general_purpose::STANDARD.decode(uri.strip_prefix("data:image/png;base64,").unwrap()).unwrap();
            assert_eq!(bytes, expected);
        }
        // A file with an image extension must not allow arbitrary file contents through.
        let invalid = root.join("invalid.png");
        std::fs::write(&invalid, b"not an image").unwrap();
        assert!(image_data_url(invalid.to_str().unwrap()).is_err());
        for path in req.images { std::fs::remove_file(path).unwrap(); }
        std::fs::remove_file(invalid).unwrap();
        std::fs::remove_dir(root).unwrap();
    }

    #[test]
    fn invalid_attachments_fail_instead_of_silent_text_fallback() {
        let backend = RemoteOpenAiBackend::new("http://127.0.0.1:11434/v1", "test", None);
        let mut req = request();
        req.images = vec!["missing-illustration-fixture.png".into()];
        assert!(backend.request_body(&req).is_err());
        req.images = vec!["x".into(); 5];
        assert!(backend.request_body(&req).is_err());
        req.images.truncate(1);
        req.messages.clear();
        assert!(backend.request_body(&req).is_err());
    }
}

impl From<reqwest::Error> for RemoteError {
    fn from(e: reqwest::Error) -> Self {
        RemoteError::Http(e.to_string())
    }
}

fn image_data_url(raw: &str) -> Result<String, RemoteError> {
    const MAX_BYTES: u64 = 16 * 1024 * 1024;
    let path = crate::subsystem::resolve_infer_image_path(raw);
    let file = std::fs::File::open(path)
        .map_err(|_| RemoteError::Image("PNG/JPEG/WebP introuvable ou illisible".into()))?;
    if !file.metadata().map_err(|_| RemoteError::Image("métadonnées illisibles".into()))?.is_file() {
        return Err(RemoteError::Image("fichier image régulier requis".into()));
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes)
        .map_err(|_| RemoteError::Image("lecture impossible".into()))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(RemoteError::Image("image supérieure à 16 Mio".into()));
    }
    let format = image::guess_format(&bytes)
        .map_err(|_| RemoteError::Image("format image inconnu".into()))?;
    let mime = match format {
        image::ImageFormat::Png => "image/png",
        image::ImageFormat::Jpeg => "image/jpeg",
        image::ImageFormat::WebP => "image/webp",
        _ => return Err(RemoteError::Image("PNG/JPEG/WebP requis".into())),
    };
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(&bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    reader.decode().map_err(|_| RemoteError::Image("image invalide ou dimensions excessives".into()))?;
    Ok(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

/// Backend distant (OpenAI-compatible chat completions).
#[derive(Clone)]
pub struct RemoteOpenAiBackend {
    pub endpoint: String,
    pub remote_model: String,
    api_key: Option<String>,
    client: reqwest::Client,
    release_ollama_after_response: bool,
}

impl RemoteOpenAiBackend {
    pub fn new(endpoint: &str, remote_model: &str, api_key: Option<String>) -> Self {
        Self {
            endpoint: endpoint.trim_end_matches('/').to_string(),
            remote_model: remote_model.to_string(),
            api_key,
            release_ollama_after_response: false,
            // Never forward an image or a credential to a redirected endpoint.
            client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build().expect("HTTP client"),
        }
    }

    /// Inférence en flux : envoie les deltas via `tx`.
    pub async fn infer_stream(
        &self,
        req: &InferRequest,
        tx: tokio::sync::mpsc::Sender<TokenEvent>,
        abort: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<(), RemoteError> {
        if self.release_ollama_after_response {
            return self.infer_ollama_transient(req, tx, abort).await;
        }
        let started = std::time::Instant::now();
        let body = self.request_body(req)?;
        let mut request = self
            .client
            .post(format!("{}/chat/completions", self.endpoint))
            .json(&body);
        if let Some(key) = &self.api_key {
            request = request.bearer_auth(key);
        }
        let resp = request.send().await?;
        if !resp.status().is_success() {
            return Err(RemoteError::Http(format!("statut {}", resp.status())));
        }

        let mut generated = 0u32;
        let mut ttft_ms: Option<f64> = None;
        let mut stream = resp.bytes_stream();
        let mut buffer = String::new();
        while let Some(chunk) = stream.next().await {
            if abort.load(std::sync::atomic::Ordering::SeqCst) {
                return Ok(()); // cancellation : la connexion est abandonnée
            }
            let chunk = chunk?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));
            // Traite les lignes SSE complètes.
            while let Some(pos) = buffer.find('\n') {
                let line = buffer[..pos].trim().to_string();
                buffer = buffer[pos + 1..].to_string();
                let Some(data) = line.strip_prefix("data:") else {
                    continue;
                };
                let data = data.trim();
                if data == "[DONE]" {
                    let total = started.elapsed().as_secs_f64() * 1000.0;
                    let ttft = ttft_ms.unwrap_or(total);
                    let decode_s = ((total - ttft) / 1000.0).max(1e-6);
                    let _ = tx
                        .send(TokenEvent::Done {
                            prompt_tokens: 0,
                            generated_tokens: generated,
                            ttft_ms: ttft,
                            tok_s: generated as f64 / decode_s,
                        })
                        .await;
                    return Ok(());
                }
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(data) {
                    if let Some(content) = v["choices"][0]["delta"]["content"].as_str() {
                        if ttft_ms.is_none() {
                            ttft_ms = Some(started.elapsed().as_secs_f64() * 1000.0);
                        }
                        generated += 1;
                        let _ = tx
                            .send(TokenEvent::Delta {
                                text: content.to_string(),
                            })
                            .await;
                    }
                }
            }
        }
        Err(RemoteError::Sse("flux fermé sans marqueur terminal".into()))
    }

    fn request_body(&self, req: &InferRequest) -> Result<serde_json::Value, RemoteError> {
        let mut messages: Vec<_> = req.messages.iter()
            .map(|m| serde_json::json!({"role": m.role, "content": m.content})).collect();
        if !req.images.is_empty() {
            if req.images.len() > 4 {
                return Err(RemoteError::Image("au plus quatre images par tour".into()));
            }
            let index = req.messages.iter().rposition(|m| m.role == "user")
                .ok_or_else(|| RemoteError::Image("message user absent".into()))?;
            let mut content = vec![serde_json::json!({"type":"text", "text":req.messages[index].content})];
            for path in &req.images {
                let data_url = image_data_url(path)?;
                content.push(serde_json::json!({"type":"image_url", "image_url":{"url":data_url}}));
            }
            messages[index]["content"] = serde_json::Value::Array(content);
        }
        Ok(serde_json::json!({
            "model": self.remote_model,
            "messages": messages,
            "max_tokens": req.params.max_tokens,
            "temperature": req.params.temperature,
            "stream": true,
        }))
    }

    /// Explicit opt-in local profile: each request releases its own model lease.
    /// Never stop the Ollama service or issue a separate global unload command.
    pub fn enable_transient_ollama(&mut self) -> Result<(), RemoteError> {
        if !crate::providers::endpoint_is_loopback(&self.endpoint) {
            return Err(RemoteError::Http("profil Ollama temporaire réservé au loopback".into()));
        }
        let url = reqwest::Url::parse(&self.endpoint).map_err(|e| RemoteError::Http(e.to_string()))?;
        if url.path().trim_end_matches('/') != "/v1" {
            return Err(RemoteError::Http("endpoint Ollama /v1 requis".into()));
        }
        self.release_ollama_after_response = true;
        Ok(())
    }

    fn ollama_request_body(&self, req: &InferRequest) -> Result<serde_json::Value, RemoteError> {
        // Reuse the validated and bounded image encoding path, preserving order.
        let mut body = self.request_body(req)?;
        for message in body["messages"].as_array_mut().unwrap() {
            if let Some(parts) = message["content"].as_array() {
                let text = parts[0]["text"].clone();
                let images: Vec<_> = parts.iter().skip(1).map(|part| {
                    let uri = part["image_url"]["url"].as_str().unwrap();
                    serde_json::Value::String(uri.split_once(',').unwrap().1.into())
                }).collect();
                message["content"] = text;
                message["images"] = serde_json::Value::Array(images);
            }
        }
        Ok(serde_json::json!({
            "model": self.remote_model, "messages": body["messages"],
            "stream": true, "keep_alive": 0, "think": false,
            "options": {"num_predict":req.params.max_tokens,
                "temperature":req.params.temperature, "num_ctx":9216}
        }))
    }

    async fn infer_ollama_transient(&self, req: &InferRequest,
        tx: tokio::sync::mpsc::Sender<TokenEvent>,
        abort: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<(), RemoteError> {
        let mut url = reqwest::Url::parse(&self.endpoint).map_err(|e| RemoteError::Http(e.to_string()))?;
        url.set_path("/api/chat"); url.set_query(None); url.set_fragment(None);
        let mut request = self.client.post(url).json(&self.ollama_request_body(req)?);
        if let Some(key) = &self.api_key { request = request.bearer_auth(key); }
        let response = request.send().await?;
        if !response.status().is_success() {
            return Err(RemoteError::Http(format!("statut {}", response.status())));
        }
        let started = std::time::Instant::now();
        let mut first_token_ms = None;
        let mut stream = response.bytes_stream();
        let mut buffer = Vec::new();
        loop {
            let chunk = tokio::select! {
                _ = tx.closed() => return Ok(()),
                chunk = stream.next() => chunk,
            };
            if abort.load(std::sync::atomic::Ordering::SeqCst) { return Ok(()); }
            let ended = chunk.is_none();
            if let Some(chunk) = chunk { buffer.extend_from_slice(&chunk?); }
            if buffer.len() > 4 * 1024 * 1024 { return Err(RemoteError::Sse("ligne Ollama excessive".into())); }
            while let Some(end) = buffer.iter().position(|b| *b == b'\n').or_else(|| (ended && !buffer.is_empty()).then_some(buffer.len())) {
                let line: Vec<_> = buffer.drain(..end).collect();
                if buffer.first() == Some(&b'\n') { buffer.remove(0); }
                if line.iter().all(|b| b.is_ascii_whitespace()) { continue; }
                let event: serde_json::Value = serde_json::from_slice(&line).map_err(|e| RemoteError::Sse(e.to_string()))?;
                if let Some(error) = event["error"].as_str() { return Err(RemoteError::Sse(error.into())); }
                if let Some(text) = event["message"]["content"].as_str().filter(|s| !s.is_empty()) {
                    first_token_ms.get_or_insert_with(|| started.elapsed().as_secs_f64() * 1000.0);
                    if tx.send(TokenEvent::Delta {text:text.into()}).await.is_err() { return Ok(()); }
                }
                if event["done"].as_bool() == Some(true) {
                    let count = event["eval_count"].as_u64().unwrap_or(0) as u32;
                    let seconds = event["eval_duration"].as_f64().unwrap_or(0.0) / 1e9;
                    let _ = tx.send(TokenEvent::Done {
                        prompt_tokens:event["prompt_eval_count"].as_u64().unwrap_or(0) as u32,
                        generated_tokens:count, ttft_ms:first_token_ms.unwrap_or(0.0),
                        tok_s:if seconds > 0.0 {count as f64 / seconds} else {0.0},
                    }).await;
                    return Ok(());
                }
            }
            if ended { return Err(RemoteError::Sse("flux Ollama fermé sans fin".into())); }
        }
    }

    /// `health()` : teste la connectivité (GET /models toléré absent).
    pub async fn health(&self) -> bool {
        let mut request = self.client.get(format!("{}/models", self.endpoint));
        if let Some(key) = &self.api_key {
            request = request.bearer_auth(key);
        }
        matches!(request.send().await, Ok(r) if r.status().is_success() || r.status().as_u16() == 404)
    }

    pub async fn list_models(&self) -> Result<Vec<String>, RemoteError> {
        let mut request = self.client.get(format!("{}/models", self.endpoint));
        if let Some(key) = &self.api_key {
            request = request.bearer_auth(key);
        }
        let resp = request.send().await?;
        if !resp.status().is_success() {
            return Err(RemoteError::Http(format!("statut {}", resp.status())));
        }
        let v: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| RemoteError::Http(e.to_string()))?;
        let ids = v
            .get("data")
            .and_then(|d| d.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| m.get("id").and_then(|i| i.as_str()).map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        Ok(ids)
    }
}
