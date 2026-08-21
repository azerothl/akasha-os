//! Client du bus : appels unaires et flux.

use crate::codec::{transport, Transport};
use crate::msg::{Frame, Intent, Status, StreamItem};
use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::io;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{mpsc, oneshot, Mutex};

#[derive(Debug, Error)]
pub enum CallError {
    #[error("transport: {0}")]
    Io(#[from] io::Error),
    #[error("encodage CBOR: {0}")]
    Cbor(String),
    #[error("statut {status:?}: {message}")]
    Status { status: Status, message: String },
    #[error("réponse inattendue du bus")]
    Protocol,
    #[error("canal fermé prématurément")]
    Closed,
}

enum Pending {
    Unary(oneshot::Sender<Result<(Status, Vec<u8>), CallError>>),
    Stream(mpsc::Sender<Result<StreamItem, CallError>>),
}

/// Client du Semantic IPC Bus.
pub struct BusClient {
    writer: Mutex<futures::stream::SplitSink<Transport, bytes::Bytes>>,
    pending: Mutex<HashMap<u64, Pending>>,
    next_id: AtomicU64,
    /// Identité logique présentée dans les intents.
    pub from: String,
}

impl BusClient {
    /// Se connecte au broker et lance la tâche de lecture.
    pub async fn connect(addr: &str, from: impl Into<String>) -> Result<Arc<Self>, CallError> {
        let stream = tokio::net::TcpStream::connect(addr).await?;
        let (sink, mut source) = transport(stream).split();
        let client = Arc::new(Self {
            writer: Mutex::new(sink),
            pending: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            from: from.into(),
        });
        let c = client.clone();
        tokio::spawn(async move { c.read_loop(&mut source).await });
        Ok(client)
    }

    async fn read_loop(&self, source: &mut futures::stream::SplitStream<Transport>) {
        while let Some(frame) = source.next().await {
            let frame = match frame {
                Ok(b) => b,
                Err(_) => break,
            };
            let frame = match Frame::decode(&frame) {
                Ok(f) => f,
                Err(_) => continue,
            };
            match frame {
                Frame::Response {
                    correlation_id,
                    status,
                    payload,
                } => {
                    let p = self.pending.lock().await.remove(&correlation_id);
                    if let Some(Pending::Unary(tx)) = p {
                        let _ = tx.send(Ok((status, payload)));
                    }
                }
                Frame::Stream(item) => {
                    let tx = {
                        let p = self.pending.lock().await;
                        match p.get(&item.correlation_id) {
                            Some(Pending::Stream(tx)) => Some(tx.clone()),
                            _ => None,
                        }
                    };
                    if let Some(tx) = tx {
                        // Ne jamais bloquer read_loop : sinon pending.lock + client
                        // en await sur un call unaire → deadlock (agents figés).
                        match tx.try_send(Ok(item)) {
                            Ok(()) => {}
                            Err(tokio::sync::mpsc::error::TrySendError::Full(v)) => {
                                tokio::spawn(async move {
                                    let _ = tx.send(v).await;
                                });
                            }
                            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {}
                        }
                    }
                }
                Frame::StreamEnd {
                    correlation_id,
                    status,
                } => {
                    let p = self.pending.lock().await.remove(&correlation_id);
                    if let Some(Pending::Stream(tx)) = p {
                        if status != Status::Ok {
                            let err = CallError::Status {
                                status,
                                message: "fin de flux en erreur".into(),
                            };
                            match tx.try_send(Err(err)) {
                                Ok(()) => {}
                                Err(tokio::sync::mpsc::error::TrySendError::Full(v)) => {
                                    tokio::spawn(async move {
                                        let _ = tx.send(v).await;
                                    });
                                }
                                Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {}
                            }
                        }
                        // drop(tx) → ferme le flux côté récepteur.
                    }
                }
                _ => { /* Lookup etc. gérés par des appels unaires internes */ }
            }
        }
        // Connexion perdue : échoue tous les appels en attente.
        let mut pend = self.pending.lock().await;
        for (_, p) in pend.drain() {
            match p {
                Pending::Unary(tx) => {
                    let _ = tx.send(Err(CallError::Closed));
                }
                Pending::Stream(tx) => {
                    let _ = tx.send(Err(CallError::Closed)).await;
                }
            }
        }
    }

    async fn send(&self, frame: Frame) -> Result<(), CallError> {
        let buf = frame.encode()?;
        self.writer.lock().await.send(Bytes::from(buf)).await?;
        Ok(())
    }

    fn next_correlation(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Unary call with an explicit `Intent.from` (bridge / multi-tenant clients).
    pub async fn call_from<Req, Resp>(
        &self,
        from: &str,
        intent: &str,
        req: &Req,
        caps: Vec<String>,
    ) -> Result<Resp, CallError>
    where
        Req: serde::Serialize,
        Resp: serde::de::DeserializeOwned,
    {
        let id = self.next_correlation();
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, Pending::Unary(tx));
        let msg = Intent {
            intent: intent.into(),
            version: 1,
            payload: crate::to_cbor(req).map_err(|e| CallError::Cbor(e.to_string()))?,
            caps,
            correlation_id: id,
            wants_stream: false,
            from: from.to_string(),
        };
        self.send(Frame::Intent(msg)).await?;
        let (status, payload) = rx.await.map_err(|_| CallError::Closed)??;
        if status != Status::Ok {
            return Err(CallError::Status {
                status,
                message: String::from_utf8_lossy(&payload).into_owned(),
            });
        }
        crate::from_cbor(&payload).map_err(|e| CallError::Cbor(e.to_string()))
    }

    /// Appel unaire typé.
    pub async fn call<Req, Resp>(
        &self,
        intent: &str,
        req: &Req,
        caps: Vec<String>,
    ) -> Result<Resp, CallError>
    where
        Req: serde::Serialize,
        Resp: serde::de::DeserializeOwned,
    {
        self.call_from(&self.from, intent, req, caps).await
    }

    /// Appel en flux : retourne un récepteur d'éléments typés.
    pub async fn call_stream<Req, Item>(
        &self,
        intent: &str,
        req: &Req,
        caps: Vec<String>,
    ) -> Result<mpsc::Receiver<Result<Item, CallError>>, CallError>
    where
        Req: serde::Serialize,
        Item: serde::de::DeserializeOwned + Send + 'static,
    {
        let id = self.next_correlation();
        let (tx, mut rx_raw) = mpsc::channel::<Result<StreamItem, CallError>>(512);
        self.pending.lock().await.insert(id, Pending::Stream(tx));
        let msg = Intent {
            intent: intent.into(),
            version: 1,
            payload: crate::to_cbor(req).map_err(|e| CallError::Cbor(e.to_string()))?,
            caps,
            correlation_id: id,
            wants_stream: true,
            from: self.from.clone(),
        };
        self.send(Frame::Intent(msg)).await?;

        let (tx_typed, rx_typed) = mpsc::channel(512);
        tokio::spawn(async move {
            while let Some(item) = rx_raw.recv().await {
                let out = item.and_then(|it| {
                    crate::from_cbor::<Item>(&it.payload)
                        .map_err(|e| CallError::Cbor(e.to_string()))
                });
                if tx_typed.send(out).await.is_err() {
                    break;
                }
            }
        });
        Ok(rx_typed)
    }

    /// Découverte : un service sert-il cet intent ?
    pub async fn lookup(&self, intent: &str) -> Result<bool, CallError> {
        let id = self.next_correlation();
        let (tx, rx) = oneshot::channel();
        // Réutilise le canal unaire : le broker répond Frame::Response.
        self.pending.lock().await.insert(id, Pending::Unary(tx));
        self.send(Frame::Intent(Intent {
            intent: format!("bus.lookup:{intent}"),
            version: 1,
            payload: Vec::new(),
            caps: vec![],
            correlation_id: id,
            wants_stream: false,
            from: self.from.clone(),
        }))
        .await?;
        let (status, payload) = rx.await.map_err(|_| CallError::Closed)??;
        if status != Status::Ok {
            return Ok(false);
        }
        crate::from_cbor::<bool>(&payload).map_err(|e| CallError::Cbor(e.to_string()))
    }
}
