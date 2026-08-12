//! Broker du bus (`aos-busd`) : routage d'intents, corrélation, découverte.
//!
//! Responsabilités :
//! - les services s'enregistrent (`Frame::Register`) avec la liste des intents
//!   servis ;
//! - les clients envoient des `Intent` adressés par nom ; le broker réécrit
//!   l'identifiant de corrélation (espace d'IDs global) et route ;
//! - `bus.lookup:<intent>` est répondu directement par le broker (découverte) ;
//! - déconnexion d'un service : ses routes tombent et les appels en attente
//!   échouent proprement (`InternalError`).

use crate::codec::{transport, Transport};
use crate::msg::{Frame, Status};
use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::io;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Mutex;

type Writer = Arc<Mutex<futures::stream::SplitSink<Transport, Bytes>>>;

#[derive(Default)]
struct BrokerState {
    conns: HashMap<u64, Writer>,
    /// intent → connexion du service qui le sert.
    routes: HashMap<String, u64>,
    /// broker corr id → (connexion cliente, corr id côté client, connexion service).
    pending: HashMap<u64, (u64, u64, u64)>,
    next_broker_id: u64,
}

/// Sert le broker sur un listener déjà lié.
pub async fn serve(listener: TcpListener) -> io::Result<()> {
    let state = Arc::new(Mutex::new(BrokerState::default()));
    let mut next_conn = 1u64;
    loop {
        let (stream, _) = listener.accept().await?;
        let conn_id = next_conn;
        next_conn += 1;
        let (sink, source) = transport(stream).split();
        let writer = Arc::new(Mutex::new(sink));
        state.lock().await.conns.insert(conn_id, writer.clone());
        let st = state.clone();
        tokio::spawn(async move {
            handle_conn(conn_id, writer, source, st.clone()).await;
            cleanup_conn(conn_id, st).await;
        });
    }
}

/// Lie un listener et sert indéfiniment.
pub async fn serve_addr(addr: &str) -> io::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    serve(listener).await
}

async fn send(writer: &Writer, frame: Frame) -> io::Result<()> {
    let buf = frame.encode()?;
    writer.lock().await.send(Bytes::from(buf)).await
}

async fn handle_conn(
    conn_id: u64,
    writer: Writer,
    mut source: futures::stream::SplitStream<Transport>,
    state: Arc<Mutex<BrokerState>>,
) {
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
            Frame::Register { intents, .. } => {
                let mut st = state.lock().await;
                for i in intents {
                    st.routes.insert(i, conn_id);
                }
            }
            Frame::Intent(mut intent) => {
                // Découverte : `bus.lookup:<intent>` répondu par le broker.
                if let Some(name) = intent.intent.strip_prefix("bus.lookup:") {
                    let available = state.lock().await.routes.contains_key(name);
                    let payload = crate::to_cbor(&available).unwrap_or_default();
                    let _ = send(
                        &writer,
                        Frame::Response {
                            correlation_id: intent.correlation_id,
                            status: Status::Ok,
                            payload,
                        },
                    )
                    .await;
                    continue;
                }
                let target = state.lock().await.routes.get(&intent.intent).copied();
                match target {
                    Some(svc) => {
                        let (broker_id, svc_writer) = {
                            let mut st = state.lock().await;
                            let broker_id = st.next_broker_id;
                            st.next_broker_id += 1;
                            st.pending
                                .insert(broker_id, (conn_id, intent.correlation_id, svc));
                            (broker_id, st.conns.get(&svc).cloned())
                        };
                        intent.correlation_id = broker_id;
                        if let Some(w) = svc_writer {
                            if send(&w, Frame::Intent(intent)).await.is_err() {
                                state.lock().await.pending.remove(&broker_id);
                            }
                        }
                    }
                    None => {
                        let _ = send(
                            &writer,
                            Frame::Response {
                                correlation_id: intent.correlation_id,
                                status: Status::NotFound,
                                payload: format!("aucun service pour {}", intent.intent)
                                    .into_bytes(),
                            },
                        )
                        .await;
                    }
                }
            }
            Frame::Response {
                correlation_id,
                status,
                payload,
            } => {
                let route = state.lock().await.pending.remove(&correlation_id);
                if let Some((client_conn, client_id, _)) = route {
                    let w = state.lock().await.conns.get(&client_conn).cloned();
                    if let Some(w) = w {
                        let _ = send(
                            &w,
                            Frame::Response {
                                correlation_id: client_id,
                                status,
                                payload,
                            },
                        )
                        .await;
                    }
                }
            }
            Frame::Stream(item) => {
                let route = state
                    .lock()
                    .await
                    .pending
                    .get(&item.correlation_id)
                    .copied();
                if let Some((client_conn, client_id, _)) = route {
                    let w = state.lock().await.conns.get(&client_conn).cloned();
                    if let Some(w) = w {
                        let _ = send(
                            &w,
                            Frame::Stream(crate::msg::StreamItem {
                                correlation_id: client_id,
                                payload: item.payload,
                            }),
                        )
                        .await;
                    }
                }
            }
            Frame::StreamEnd {
                correlation_id,
                status,
            } => {
                let route = state.lock().await.pending.remove(&correlation_id);
                if let Some((client_conn, client_id, _)) = route {
                    let w = state.lock().await.conns.get(&client_conn).cloned();
                    if let Some(w) = w {
                        let _ = send(
                            &w,
                            Frame::StreamEnd {
                                correlation_id: client_id,
                                status,
                            },
                        )
                        .await;
                    }
                }
            }
            Frame::Lookup { .. } | Frame::LookupResult { .. } => {
                // Réservé — la découverte passe par `bus.lookup:`.
            }
        }
    }
}

async fn cleanup_conn(conn_id: u64, state: Arc<Mutex<BrokerState>>) {
    let orphaned: Vec<(u64, Writer)> = {
        let mut st = state.lock().await;
        st.conns.remove(&conn_id);
        // Routes tenues par cette connexion (service parti).
        st.routes.retain(|_, v| *v != conn_id);
        // Appels dont le *client* est parti : oubliés.
        st.pending.retain(|_, (client, _, _)| *client != conn_id);
        // Appels routés vers ce *service* mort : échoués proprement.
        let orphaned_ids: Vec<u64> = st
            .pending
            .iter()
            .filter(|(_, (_, _, svc))| *svc == conn_id)
            .map(|(k, _)| *k)
            .collect();
        let mut orphaned = Vec::new();
        for id in orphaned_ids {
            if let Some((client, client_id, _)) = st.pending.remove(&id) {
                if let Some(w) = st.conns.get(&client) {
                    orphaned.push((client_id, w.clone()));
                }
            }
        }
        orphaned
    };
    for (client_id, w) in orphaned {
        let _ = send(
            &w,
            Frame::Response {
                correlation_id: client_id,
                status: Status::InternalError,
                payload: b"service d\xc3\xa9connect\xc3\xa9".to_vec(),
            },
        )
        .await;
    }
}
