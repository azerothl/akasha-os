//! Côté service : enregistrement d'intents et handlers.

use crate::codec::{transport, Transport};
use crate::msg::{Frame, Intent, Status, StreamItem};
use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("transport: {0}")]
    Io(#[from] io::Error),
    #[error("encodage CBOR: {0}")]
    Cbor(String),
}

/// Résultat d'un handler : réponse unaire ou flux d'éléments.
pub enum HandlerResult {
    /// Réponse unique (statut + payload CBOR déjà encodé).
    Unary(Status, Vec<u8>),
    /// Flux : le service envoie des [`StreamItem`] puis termine par
    /// [`StreamHandle::finish`].
    Stream,
}

/// Contexte passé à un handler.
pub struct IntentCtx {
    pub intent: Intent,
    responder: Responder,
}

impl IntentCtx {
    /// Décode le payload typé.
    pub fn payload<T: serde::de::DeserializeOwned>(&self) -> Result<T, ServiceError> {
        crate::from_cbor(&self.intent.payload).map_err(|e| ServiceError::Cbor(e.to_string()))
    }

    /// Réponse unaire typée.
    pub async fn respond<T: serde::Serialize>(
        self,
        status: Status,
        value: &T,
    ) -> Result<(), ServiceError> {
        let payload = crate::to_cbor(value).map_err(|e| ServiceError::Cbor(e.to_string()))?;
        self.responder.respond_raw(status, payload).await
    }

    /// Réponse unaire d'erreur (message texte en payload).
    pub async fn respond_error(self, status: Status, message: &str) -> Result<(), ServiceError> {
        self.responder
            .respond_raw(status, message.as_bytes().to_vec())
            .await
    }

    /// Ouvre un flux de réponse.
    pub fn open_stream(&self) -> StreamHandle {
        StreamHandle {
            correlation_id: self.intent.correlation_id,
            writer: self.responder.writer.clone(),
        }
    }
}

struct Responder {
    correlation_id: u64,
    writer: Arc<Mutex<futures::stream::SplitSink<Transport, Bytes>>>,
}

impl Responder {
    async fn respond_raw(&self, status: Status, payload: Vec<u8>) -> Result<(), ServiceError> {
        let frame = Frame::Response {
            correlation_id: self.correlation_id,
            status,
            payload,
        };
        let buf = frame.encode()?;
        self.writer.lock().await.send(Bytes::from(buf)).await?;
        Ok(())
    }
}

/// Handle de flux côté service.
pub struct StreamHandle {
    correlation_id: u64,
    writer: Arc<Mutex<futures::stream::SplitSink<Transport, Bytes>>>,
}

impl StreamHandle {
    async fn send_frame(&self, frame: Frame) -> Result<(), ServiceError> {
        let buf = frame.encode()?;
        self.writer.lock().await.send(Bytes::from(buf)).await?;
        Ok(())
    }

    /// Envoie un élément typé dans le flux.
    pub async fn send<T: serde::Serialize>(&self, item: &T) -> Result<(), ServiceError> {
        let payload = crate::to_cbor(item).map_err(|e| ServiceError::Cbor(e.to_string()))?;
        self.send_frame(Frame::Stream(StreamItem {
            correlation_id: self.correlation_id,
            payload,
        }))
        .await
    }

    /// Termine le flux avec un statut.
    pub async fn finish(self, status: Status) -> Result<(), ServiceError> {
        self.send_frame(Frame::StreamEnd {
            correlation_id: self.correlation_id,
            status,
        })
        .await
    }
}

type Handler = Arc<dyn Fn(IntentCtx) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

/// Point d'entrée d'un service sur le bus.
pub struct BusService {
    name: String,
    handlers: HashMap<String, Handler>,
}

impl BusService {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            handlers: HashMap::new(),
        }
    }

    /// Enregistre un handler pour un intent.
    pub fn on<F, Fut>(&mut self, intent: &str, handler: F) -> &mut Self
    where
        F: Fn(IntentCtx) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.handlers
            .insert(intent.into(), Arc::new(move |ctx| Box::pin(handler(ctx))));
        self
    }

    /// Connecte au broker, enregistre les intents et sert jusqu'à fermeture.
    pub async fn serve(self, addr: &str) -> Result<(), ServiceError> {
        let stream = tokio::net::TcpStream::connect(addr).await?;
        let (sink, mut source) = transport(stream).split();
        let writer = Arc::new(Mutex::new(sink));

        let intents: Vec<String> = self.handlers.keys().cloned().collect();
        let reg = Frame::Register {
            service_name: self.name.clone(),
            intents,
        }
        .encode()?;
        writer.lock().await.send(Bytes::from(reg)).await?;

        let handlers = Arc::new(self.handlers);
        while let Some(frame) = source.next().await {
            let frame = match frame {
                Ok(b) => b,
                Err(_) => break,
            };
            let frame = match Frame::decode(&frame) {
                Ok(f) => f,
                Err(_) => continue,
            };
            if let Frame::Intent(intent) = frame {
                let writer = writer.clone();
                let handlers = handlers.clone();
                tokio::spawn(async move {
                    let ctx = IntentCtx {
                        responder: Responder {
                            correlation_id: intent.correlation_id,
                            writer,
                        },
                        intent,
                    };
                    match handlers.get(&ctx.intent.intent) {
                        Some(h) => h(ctx).await,
                        None => {
                            let _ = ctx
                                .respond_error(Status::NotFound, "intent non servi")
                                .await;
                        }
                    }
                });
            }
        }
        Ok(())
    }
}
