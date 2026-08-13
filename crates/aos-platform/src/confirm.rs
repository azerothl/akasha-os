//! Confirmation bloquante (§9.4, F-SEC-07) : `require_confirmation` suspend
//! l'action, notifie l'UI (Control bar), timeout → refus audité (fail-closed).

use aos_proto::PendingConfirmation;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex};

/// Gestionnaire de confirmations en attente.
pub struct ConfirmManager {
    pending: Mutex<HashMap<String, PendingInner>>,
    /// Abonnés (UI Control bar) — diffusion des nouvelles confirmations.
    subscribers: Mutex<Vec<tokio::sync::mpsc::Sender<PendingConfirmation>>>,
    next_id: Mutex<u64>,
    default_timeout_sec: u64,
}

struct PendingInner {
    pub info: PendingConfirmation,
    pub respond: Option<oneshot::Sender<bool>>,
}

impl ConfirmManager {
    pub fn new(default_timeout_sec: u64) -> Arc<Self> {
        Arc::new(Self {
            pending: Mutex::new(HashMap::new()),
            subscribers: Mutex::new(Vec::new()),
            next_id: Mutex::new(1),
            default_timeout_sec,
        })
    }

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }

    /// Crée une confirmation bloquante ; retourne (id, futur de réponse).
    /// La réponse vaut `false` en cas de timeout (fail-closed).
    pub async fn ask(
        self: &Arc<Self>,
        actor: String,
        action: String,
        target: String,
        reason: String,
        timeout_sec: Option<u64>,
    ) -> (String, oneshot::Receiver<bool>) {
        let id = {
            let mut n = self.next_id.lock().await;
            let id = format!("confirm-{}", *n);
            *n += 1;
            id
        };
        let timeout = timeout_sec.unwrap_or(self.default_timeout_sec);
        let info = PendingConfirmation {
            id: id.clone(),
            actor,
            action,
            target,
            reason,
            deadline_ts_ms: Self::now_ms() + timeout * 1000,
        };
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(
            id.clone(),
            PendingInner {
                info: info.clone(),
                respond: Some(tx),
            },
        );
        // Notifie les abonnés (Control bar).
        let mut subs = self.subscribers.lock().await;
        subs.retain(|tx| !tx.is_closed());
        for tx in subs.iter() {
            let _ = tx.send(info.clone()).await;
        }
        // Timeout fail-closed.
        let this = self.clone();
        let id2 = id.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(timeout)).await;
            let mut pend = this.pending.lock().await;
            if let Some(inner) = pend.remove(&id2) {
                if let Some(tx) = inner.respond {
                    let _ = tx.send(false); // timeout → refus
                }
            }
        });
        (id, rx)
    }

    /// Réponse humaine (`confirm.respond`).
    pub async fn respond(&self, id: &str, approved: bool) -> bool {
        let mut pend = self.pending.lock().await;
        if let Some(inner) = pend.remove(id) {
            if let Some(tx) = inner.respond {
                let _ = tx.send(approved);
                return true;
            }
        }
        false
    }

    /// Abonnement au flux des nouvelles confirmations.
    pub async fn subscribe(&self) -> tokio::sync::mpsc::Receiver<PendingConfirmation> {
        let (tx, rx) = tokio::sync::mpsc::channel(32);
        self.subscribers.lock().await.push(tx);
        rx
    }

    pub async fn pending_list(&self) -> Vec<PendingConfirmation> {
        self.pending
            .lock()
            .await
            .values()
            .map(|p| p.info.clone())
            .collect()
    }
}
