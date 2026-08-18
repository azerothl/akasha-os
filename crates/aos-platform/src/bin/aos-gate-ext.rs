//! `aos-gate-ext` — gate des extensions agent (F-EXT-01…06).
//!
//! Prérequis : bus + platformd (+ agentd pour hot-grant) démarrés.
//! Usage : `aos-gate-ext [bus_addr]`

use aos_ipc::BusClient;
use aos_proto::*;

struct Check {
    name: &'static str,
    ok: bool,
    detail: String,
}

#[tokio::main]
async fn main() {
    let bus_addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| format!("127.0.0.1:{}", aos_ipc::DEFAULT_BUS_PORT));
    let bus = BusClient::connect(&bus_addr, "gate-ext")
        .await
        .expect("bus");

    let mut checks = Vec::new();

    // 1. skill.create (humain) + list
    let skill_name = format!("gate-ext-{}", std::process::id() % 10000);
    let create = bus
        .call::<SkillCreateRequest, SkillInfo>(
            "skill.create",
            &SkillCreateRequest {
                name: skill_name.clone(),
                description: "gate".into(),
                when_to_use: "test".into(),
                tools: vec!["notes.list".into()],
                required_caps: vec![],
                body: "Lister les notes.".into(),
                actor: "human:gate".into(),
                actor_caps: vec![],
            },
            vec![],
        )
        .await;
    checks.push(Check {
        name: "skill.create (humain)",
        ok: create.is_ok(),
        detail: match &create {
            Ok(i) => format!("ok {}", i.name),
            Err(e) => e.to_string(),
        },
    });

    let listed = bus
        .call::<(), Vec<SkillInfo>>("skill.list", &(), vec![])
        .await
        .unwrap_or_default();
    checks.push(Check {
        name: "skill.list voit la skill",
        ok: listed.iter().any(|s| s.name == skill_name),
        detail: format!("{} skills", listed.len()),
    });

    // 2. trust low → skill.create agent refusé
    let _ = bus
        .call::<TrustSetRequest, bool>(
            "trust.set",
            &TrustSetRequest {
                agent_id: "agent-gate-low".into(),
                score: 0.1,
            },
            vec![],
        )
        .await;
    let denied = bus
        .call::<SkillCreateRequest, SkillInfo>(
            "skill.create",
            &SkillCreateRequest {
                name: format!("{skill_name}-low"),
                description: "x".into(),
                when_to_use: String::new(),
                tools: vec![],
                required_caps: vec![],
                body: "x".into(),
                actor: "agent:agent-gate-low".into(),
                actor_caps: vec![],
            },
            vec![],
        )
        .await;
    checks.push(Check {
        name: "skill.create Low → deny",
        ok: denied.is_err(),
        detail: format!("{denied:?}"),
    });

    // 3. module.scaffold + package (script)
    let mod_name = format!("gatemod{}", std::process::id() % 1000);
    let scaffold = bus
        .call::<ModuleScaffoldRequest, ModuleScaffoldResponse>(
            "module.scaffold",
            &ModuleScaffoldRequest {
                name: mod_name.clone(),
                kind: "script".into(),
                description: "gate script module".into(),
                tools: vec![],
                required_caps: vec![format!("fs.write:/documents/{mod_name}/**")],
                source: String::new(),
                ui: String::new(),
                actor: "human:gate".into(),
                actor_caps: vec![],
            },
            vec![],
        )
        .await;
    checks.push(Check {
        name: "module.scaffold script",
        ok: scaffold.is_ok(),
        detail: format!("{scaffold:?}"),
    });

    let packaged = bus
        .call::<ModulePackageRequest, ModulePackageResponse>(
            "module.package",
            &ModulePackageRequest {
                name: mod_name.clone(),
                actor: "human:gate".into(),
                actor_caps: vec![],
            },
            vec![],
        )
        .await;
    let package_ok = packaged.is_ok();
    checks.push(Check {
        name: "module.package (ext-rt)",
        ok: package_ok,
        detail: match &packaged {
            Ok(p) => format!("{} {}", p.package_dir, p.hash),
            Err(e) => e.to_string(),
        },
    });

    if let Ok(pkg) = &packaged {
        let ui_path = std::path::Path::new(&pkg.package_dir).join("ui/index.html");
        let (ui_ok, ui_detail) = match std::fs::read(&ui_path) {
            Ok(raw) => match aos_proto::decl_ui::DeclUiDocument::parse_json(&raw) {
                Ok(doc) => {
                    let kinds = doc.collect_widget_kinds();
                    let beyond_heading = kinds
                        .iter()
                        .any(|k| k != "heading" && k != "column" && k != "row");
                    (beyond_heading, format!("kinds={kinds:?}"))
                }
                Err(e) => (false, e.to_string()),
            },
            Err(e) => (false, e.to_string()),
        };
        checks.push(Check {
            name: "declarative_ui package ui",
            ok: ui_ok,
            detail: ui_detail,
        });
    }

    // 4. module.install sans cap agent → deny
    if let Ok(pkg) = &packaged {
        let deny_install = bus
            .call::<ModuleInstallRequest, ModuleInfo>(
                "module.install",
                &ModuleInstallRequest {
                    source_dir: pkg.package_dir.clone(),
                    approved_caps: None,
                    actor: "agent:agent-gate-low".into(),
                    actor_caps: vec![],
                },
                vec![],
            )
            .await;
        checks.push(Check {
            name: "module.install agent sans cap → deny",
            ok: deny_install.is_err(),
            detail: format!("{deny_install:?}"),
        });

        // Install humain OK (revue explicite des caps — pas d'auto-approve)
        let required = aos_platform::module_rt::ModuleRuntime::peek_required_caps(
            std::path::Path::new(&pkg.package_dir),
        )
        .map(|(_, caps)| caps)
        .unwrap_or_default();
        let human_install = bus
            .call::<ModuleInstallRequest, ModuleInfo>(
                "module.install",
                &ModuleInstallRequest {
                    source_dir: pkg.package_dir.clone(),
                    approved_caps: Some(required),
                    actor: "human:gate".into(),
                    actor_caps: vec![],
                },
                vec![],
            )
            .await;
        checks.push(Check {
            name: "module.install humain",
            ok: human_install.is_ok(),
            detail: format!("{human_install:?}"),
        });
    } else {
        checks.push(Check {
            name: "module.install agent sans cap → deny",
            ok: false,
            detail: "package manquant (ext-rt.wasm ?)".into(),
        });
        checks.push(Check {
            name: "module.install humain",
            ok: false,
            detail: "package manquant".into(),
        });
    }

    // 5. module.describe catalogue
    let desc = bus
        .call::<ModuleIdRequest, serde_json::Value>(
            "module.describe",
            &ModuleIdRequest {
                module: "notes".into(),
            },
            vec![],
        )
        .await;
    checks.push(Check {
        name: "module.describe notes",
        ok: desc
            .as_ref()
            .ok()
            .and_then(|v| v.get("manifest"))
            .is_some(),
        detail: format!("{desc:?}").chars().take(120).collect(),
    });

    // 6. cap.request Medium → confirm path (on pose High pour auto)
    let _ = bus
        .call::<TrustSetRequest, bool>(
            "trust.set",
            &TrustSetRequest {
                agent_id: "agent-gate-high".into(),
                score: 0.9,
            },
            vec![],
        )
        .await;
    let grant = bus
        .call::<CapRequestRequest, CapRequestOutcome>(
            "cap.request",
            &CapRequestRequest {
                agent_id: "agent-gate-high".into(),
                cap: "tool.invoke:notes".into(),
                reason: "gate".into(),
            },
            vec![],
        )
        .await;
    checks.push(Check {
        name: "cap.request High non-critique → Granted",
        ok: matches!(grant, Ok(CapRequestOutcome::Granted)),
        detail: format!("{grant:?}"),
    });

    // Cleanup skill
    let _ = bus
        .call::<SkillNameRequest, bool>(
            "skill.uninstall",
            &SkillNameRequest {
                name: skill_name,
                actor: "human:gate".into(),
                actor_caps: vec![],
            },
            vec![],
        )
        .await;

    println!("=== aos-gate-ext ===");
    let mut failed = 0;
    for c in &checks {
        let mark = if c.ok { "PASS" } else { "FAIL" };
        if !c.ok {
            failed += 1;
        }
        println!("[{mark}] {} — {}", c.name, c.detail);
    }
    println!(
        "Résultat : {}/{} OK",
        checks.len() - failed,
        checks.len()
    );
    std::process::exit(if failed == 0 { 0 } else { 1 });
}
