//! `aos-auditd` — service d'audit autonome (P4.4).
//!
//! Possède le journal append-only signé (§9.3, §12). Séparé en processus
//! distinct pour l'isolation de panne (Gate P4) : le tuer n'affecte ni le
//! Model Subsystem ni l'UI. Usage : `aos-auditd [bus_addr] [audit_dir]`.

use aos_ipc::BusService;
use aos_platform::audit::AuditJournal;
use aos_proto::{AuditAppendRequest, AuditQueryRequest};
use std::sync::{Arc, Mutex};

#[tokio::main]
async fn main() {
    let bus_addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| format!("127.0.0.1:{}", aos_ipc::DEFAULT_BUS_PORT));
    let audit_dir = std::env::args().nth(2).unwrap_or_else(|| "var/audit".into());

    let journal = Arc::new(Mutex::new(
        AuditJournal::open(&audit_dir).expect("ouverture du journal d'audit"),
    ));
    let mut svc = BusService::new("auditd");

    // --- audit.append ---
    {
        let j = journal.clone();
        svc.on("audit.append", move |ctx| {
            let j = j.clone();
            async move {
                match ctx.payload::<AuditAppendRequest>() {
                    Ok(req) => {
                        let ev = j.lock().unwrap().append(req);
                        let _ = ctx.respond(aos_ipc::msg::Status::Ok, &ev.seq).await;
                    }
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                    }
                }
            }
        });
    }

    // --- auditd.query (journal canonique) ---
    {
        let j = journal.clone();
        svc.on("auditd.query", move |ctx| {
            let j = j.clone();
            async move {
                match ctx.payload::<AuditQueryRequest>() {
                    Ok(req) => {
                        let events = j.lock().unwrap().query(&req);
                        let _ = ctx.respond(aos_ipc::msg::Status::Ok, &events).await;
                    }
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                    }
                }
            }
        });
    }

    // --- auditd.verify (intégrité de la chaîne) ---
    {
        let j = journal.clone();
        svc.on("auditd.verify", move |ctx| {
            let j = j.clone();
            async move {
                let ok = j.lock().unwrap().verify().is_ok();
                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &ok).await;
            }
        });
    }

    eprintln!("[aos-auditd] journal d'audit prêt sur {bus_addr} (dir {audit_dir})");
    let _ = svc.serve(&bus_addr).await;
}
