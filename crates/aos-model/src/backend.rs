//! Backend distant OpenAI-compatible (P3.1, §3.3) : client HTTP/SSE,
//! clé API via le service de secrets (jamais exposée aux agents, §9.2).

use aos_proto::{InferRequest, TokenEvent};
use futures::StreamExt;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RemoteError {
    #[error("http: {0}")]
    Http(String),
    #[error("flux SSE: {0}")]
    Sse(String),
}

impl From<reqwest::Error> for RemoteError {
    fn from(e: reqwest::Error) -> Self {
        RemoteError::Http(e.to_string())
    }
}

/// Backend distant (OpenAI-compatible chat completions).
#[derive(Clone)]
pub struct RemoteOpenAiBackend {
    pub endpoint: String,
    pub remote_model: String,
    api_key: Option<String>,
    client: reqwest::Client,
}

impl RemoteOpenAiBackend {
    pub fn new(endpoint: &str, remote_model: &str, api_key: Option<String>) -> Self {
        Self {
            endpoint: endpoint.trim_end_matches('/').to_string(),
            remote_model: remote_model.to_string(),
            api_key,
            client: reqwest::Client::new(),
        }
    }

    /// Inférence en flux : envoie les deltas via `tx`.
    pub async fn infer_stream(
        &self,
        req: &InferRequest,
        tx: tokio::sync::mpsc::Sender<TokenEvent>,
        abort: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<(), RemoteError> {
        let started = std::time::Instant::now();
        let body = serde_json::json!({
            "model": self.remote_model,
            "messages": req.messages.iter().map(|m| serde_json::json!({"role": m.role, "content": m.content})).collect::<Vec<_>>(),
            "max_tokens": req.params.max_tokens,
            "temperature": req.params.temperature,
            "stream": true,
        });
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
        Ok(())
    }

    /// `health()` : teste la connectivité (GET /models toléré absent).
    pub async fn health(&self) -> bool {
        let mut request = self.client.get(format!("{}/models", self.endpoint));
        if let Some(key) = &self.api_key {
            request = request.bearer_auth(key);
        }
        matches!(request.send().await, Ok(r) if r.status().is_success() || r.status().as_u16() == 404)
    }
}
