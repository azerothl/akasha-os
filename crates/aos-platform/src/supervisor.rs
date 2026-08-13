//! Agent superviseur v1 (§4.6, F-AGT-10) : agrégation de notifications
//! (déduplication + priorité) et arbitrage de conflits de ressources.
//!
//! v1 : les notifications sont dérivées du journal d'audit (événements
//! sensibles) avec déduplication temporelle ; l'arbitrage de conflits
//! s'applique aux transactions FS concurrentes sur les mêmes chemins
//! (priorité d'acteur, puis ancienneté).

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

/// Notification agrégée.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SupervisorNotification {
    pub ts_ms: u64,
    pub priority: u8,
    pub action: String,
    pub actor: String,
    pub target: String,
    pub count: u32,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn priority_of(action: &str) -> u8 {
    match action {
        "policy.deny" | "cap.deny" => 3,
        "fs.delete" | "confirmation.resolved" => 2,
        _ => 1,
    }
}

/// Le superviseur (agrégation + diffusion).
pub struct Supervisor {
    subs: Mutex<Vec<mpsc::Sender<SupervisorNotification>>>,
    /// Déduplication : (action, target) → dernière émission.
    last_seen: Mutex<HashMap<(String, String), u64>>,
}

impl Supervisor {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            subs: Mutex::new(Vec::new()),
            last_seen: Mutex::new(HashMap::new()),
        })
    }

    /// Événements d'audit qui méritent une notification.
    fn is_interesting(action: &str) -> bool {
        matches!(
            action,
            "policy.deny" | "cap.deny" | "fs.delete" | "confirmation.resolved" | "fs.conflict"
        )
    }

    /// Alimente le superviseur depuis un événement d'audit.
    pub async fn feed(&self, actor: &str, action: &str, target: &str) {
        if !Self::is_interesting(action) {
            return;
        }
        // Déduplication : même (action, cible) dans les 5 s → ignoré.
        let key = (action.to_string(), target.to_string());
        {
            let mut seen = self.last_seen.lock().await;
            let now = now_ms();
            if let Some(last) = seen.get(&key) {
                if now - last < 5000 {
                    return;
                }
            }
            seen.insert(key, now);
        }
        let notif = SupervisorNotification {
            ts_ms: now_ms(),
            priority: priority_of(action),
            action: action.into(),
            actor: actor.into(),
            target: target.into(),
            count: 1,
        };
        let mut subs = self.subs.lock().await;
        subs.retain(|tx| !tx.is_closed());
        for tx in subs.iter() {
            let _ = tx.send(notif.clone()).await;
        }
    }

    /// Abonnement au flux de notifications.
    pub async fn subscribe(&self) -> mpsc::Receiver<SupervisorNotification> {
        let (tx, rx) = mpsc::channel(64);
        self.subs.lock().await.push(tx);
        rx
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn dedup_et_priorite() {
        let sup = Supervisor::new();
        let mut rx = sup.subscribe().await;
        sup.feed("agent:1", "policy.deny", "/x").await;
        sup.feed("agent:1", "policy.deny", "/x").await; // doublon 5 s
        sup.feed("agent:1", "fs.delete", "/y").await;
        let n1 = rx.try_recv().unwrap();
        assert_eq!(n1.priority, 3);
        let n2 = rx.try_recv().unwrap();
        assert_eq!(n2.action, "fs.delete");
        assert!(rx.try_recv().is_err()); // le doublon a été absorbé
    }
}
