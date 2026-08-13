//! `aos-capkd` — noyau de capacités (P4.2, ADR 0001).
//!
//! Point d'application de confiance unique : toutes les vérifications de
//! capacités passent par ce service ; une révocation y est immédiatement
//! globale. Usage : `aos-capkd [bus_addr]`.

use aos_capkd::CapKernel;
use aos_ipc::BusService;
use aos_proto::{
    CapCheckRequest, CapCheckResponse, CapDeriveRequest, CapGrantRequest, CapInfo, CapListRequest,
    CapMintRequest, CapMintResponse, CapRevokeRequest,
};
use std::sync::{Arc, Mutex};

#[tokio::main]
async fn main() {
    let bus_addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| format!("127.0.0.1:{}", aos_ipc::DEFAULT_BUS_PORT));

    let kernel = Arc::new(Mutex::new(CapKernel::new()));
    let mut svc = BusService::new("capkd");

    // --- cap.mint ---
    {
        let k = kernel.clone();
        svc.on("cap.mint", move |ctx| {
            let k = k.clone();
            async move {
                match ctx.payload::<CapMintRequest>() {
                    Ok(req) => {
                        let cap_id = k
                            .lock()
                            .unwrap()
                            .mint(&req.holder, &req.object, &req.rights);
                        let _ = ctx
                            .respond(aos_ipc::msg::Status::Ok, &CapMintResponse { cap_id })
                            .await;
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

    // --- cap.derive ---
    {
        let k = kernel.clone();
        svc.on("cap.derive", move |ctx| {
            let k = k.clone();
            async move {
                match ctx.payload::<CapDeriveRequest>() {
                    Ok(req) => {
                        let r = k
                            .lock()
                            .unwrap()
                            .derive(&req.holder, req.parent, &req.rights);
                        match r {
                            Ok(cap_id) => {
                                let _ = ctx
                                    .respond(aos_ipc::msg::Status::Ok, &CapMintResponse { cap_id })
                                    .await;
                            }
                            Err(e) => {
                                let _ = ctx
                                    .respond_error(aos_ipc::msg::Status::PermissionDenied, &e)
                                    .await;
                            }
                        }
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

    // --- cap.grant ---
    {
        let k = kernel.clone();
        svc.on("cap.grant", move |ctx| {
            let k = k.clone();
            async move {
                match ctx.payload::<CapGrantRequest>() {
                    Ok(req) => {
                        let r = k.lock().unwrap().grant(&req.holder, req.cap, &req.to);
                        match r {
                            Ok(cap_id) => {
                                let _ = ctx
                                    .respond(aos_ipc::msg::Status::Ok, &CapMintResponse { cap_id })
                                    .await;
                            }
                            Err(e) => {
                                let _ = ctx
                                    .respond_error(aos_ipc::msg::Status::PermissionDenied, &e)
                                    .await;
                            }
                        }
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

    // --- cap.revoke (unitaire ou arbre) ---
    {
        let k = kernel.clone();
        svc.on("cap.revoke", move |ctx| {
            let k = k.clone();
            async move {
                match ctx.payload::<CapRevokeRequest>() {
                    Ok(req) => {
                        let r = k.lock().unwrap().revoke(&req.holder, req.cap, req.tree);
                        match r {
                            Ok(n) => {
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &n).await;
                            }
                            Err(e) => {
                                let _ = ctx
                                    .respond_error(aos_ipc::msg::Status::PermissionDenied, &e)
                                    .await;
                            }
                        }
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

    // --- cap.check ---
    {
        let k = kernel.clone();
        svc.on("cap.check", move |ctx| {
            let k = k.clone();
            async move {
                match ctx.payload::<CapCheckRequest>() {
                    Ok(req) => {
                        let (allowed, reason) = k.lock().unwrap().check_object(
                            &req.holder,
                            req.cap,
                            &req.rights,
                            req.object.as_deref(),
                        );
                        let _ = ctx
                            .respond(
                                aos_ipc::msg::Status::Ok,
                                &CapCheckResponse { allowed, reason },
                            )
                            .await;
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

    // --- cap.list ---
    {
        let k = kernel.clone();
        svc.on("cap.list", move |ctx| {
            let k = k.clone();
            async move {
                match ctx.payload::<CapListRequest>() {
                    Ok(req) => {
                        let caps: Vec<CapInfo> = k
                            .lock()
                            .unwrap()
                            .list(&req.holder)
                            .into_iter()
                            .map(|(cap_id, object, rights)| CapInfo {
                                cap_id,
                                object,
                                rights,
                                holder: req.holder.clone(),
                            })
                            .collect();
                        let _ = ctx.respond(aos_ipc::msg::Status::Ok, &caps).await;
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

    eprintln!("[aos-capkd] noyau de capacités prêt sur {bus_addr}");
    let _ = svc.serve(&bus_addr).await;
}
