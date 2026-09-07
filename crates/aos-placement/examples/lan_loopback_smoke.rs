//! End-to-end smoke test for the authenticated Akasha LAN transport.
//!
//! This uses two loopback identities and exercises the same handshake and
//! encrypted control channel used by `aos-modeld`, without requiring a second
//! machine or a model file.

use aos_placement::{
    DistributedWork, LanNode, LanPairingRegistry, LanSessionKey, LanTcpListener,
    LanTcpTransport, LanWorkMessage, NodeTrust,
};

#[tokio::main]
async fn main() -> Result<(), String> {
    let server = LanTcpListener::bind("worker", "127.0.0.1:0", &LanPairingRegistry::default())
        .await?;
    let address = server.local_addr()?.to_string();
    let mut registry = LanPairingRegistry::default();
    registry.discover(LanNode {
        node_id: "worker".into(),
        display_name: "loopback worker".into(),
        address: address.clone(),
        public_key_fingerprint: "sha256:loopback-worker".into(),
        trust: NodeTrust::Unpaired,
        capabilities: vec!["sensitive-data".into()],
    });
    registry.pair("worker", "sha256:loopback-worker")?;

    let work = DistributedWork {
        work_id: "loopback-smoke".into(),
        model_id: "smoke-model".into(),
        shard_ids: vec![0],
        allow_sensitive_data: false,
        encrypted_transport: true,
    };
    let key = LanSessionKey::from_bytes(&[7; 32])?;
    let server_registry = registry.clone();
    let server_work = work.clone();
    let server_key = key.clone();
    let server_task = tokio::spawn(async move {
        let mut transport = server
            .accept_authenticated("worker", &server_work, server_key)
            .await?;
        match transport.receive_message(&server_work).await? {
            LanWorkMessage::Heartbeat { work_id } if work_id == server_work.work_id => {
                transport
                    .send_message(
                        &LanWorkMessage::Ack {
                            work_id: server_work.work_id.clone(),
                            operation: "heartbeat".into(),
                        },
                        &server_work,
                    )
                    .await?;
                Ok::<(), String>(())
            }
            _ => Err("message inattendu dans le smoke test LAN".into()),
        }
    });

    let mut client = LanTcpTransport::connect_authenticated(
        "coordinator",
        "worker",
        &address,
        &server_registry,
        &work,
        key,
    )
    .await?;
    client
        .send_message(
            &LanWorkMessage::Heartbeat {
                work_id: work.work_id.clone(),
            },
            &work,
        )
        .await?;
    match client.receive_message(&work).await? {
        LanWorkMessage::Ack { operation, .. } if operation == "heartbeat" => {}
        _ => return Err("accusé de réception LAN inattendu".into()),
    }
    server_task
        .await
        .map_err(|e| format!("tâche LAN interrompue: {e}"))??;
    println!("LAN loopback smoke: OK (handshake, chiffrement, heartbeat, ack)");
    Ok(())
}
