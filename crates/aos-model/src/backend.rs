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
