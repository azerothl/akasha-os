//! Authoring de modules : scaffold script/rust, package (ext-rt), compile wasm32.

use aos_proto::{
    decl_ui::{default_document, document_to_json, DeclUiDocument},
    ModuleCompileResponse, ModuleManifest, ModulePackageResponse, ModulePermissions, ModuleScaffoldRequest,
    ModuleScaffoldResponse, ModuleTool, ModuleUi,
};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;

const FORBIDDEN_IN_SOURCE: &[&str] = &[
    "unsafe",
    "std::fs",
    "std::net",
    "std::process",
    "include!",
    "include_bytes!",
    "include_str!",
    "std::env",
    "Command::",
];

#[derive(Debug, thiserror::Error)]
pub enum CompileError {
    #[error("nom invalide: {0}")]
    BadName(String),
    #[error("source absente: {0}")]
    Missing(String),
    #[error("contrôle statique: {0}")]
    StaticCheck(String),
    #[error("toolchain: {0}")]
    Toolchain(String),
    #[error("compile: {0}")]
    Build(String),
    #[error("io: {0}")]
    Io(String),
    #[error("{0}")]
    Other(String),
}

/// Chemins d'authoring sous `var/modules`.
pub struct ModuleAuthor {
    pub modules_dir: PathBuf,
    pub src_dir: PathBuf,
    pub build_dir: PathBuf,
    pub packages_dir: PathBuf,
    pub ext_rt_wasm: PathBuf,
    pub sdk_path: PathBuf,
}

impl ModuleAuthor {
    pub fn open(modules_dir: impl AsRef<Path>) -> Result<Self, CompileError> {
        let modules_dir = modules_dir.as_ref().to_path_buf();
        let src_dir = modules_dir.join("src");
        let build_dir = modules_dir.join("build");
        let packages_dir = modules_dir.join("packages");
        for d in [&src_dir, &build_dir, &packages_dir] {
            std::fs::create_dir_all(d).map_err(|e| CompileError::Io(e.to_string()))?;
        }
        let ext_rt_wasm = resolve_ext_rt_wasm(&modules_dir);
        let sdk_path = resolve_sdk_path();
        Ok(Self {
            modules_dir,
            src_dir,
            build_dir,
            packages_dir,
            ext_rt_wasm,
            sdk_path,
        })
    }

    pub fn validate_name(name: &str) -> Result<(), CompileError> {
        let ok = name.len() >= 2
            && name.len() <= 32
            && name
                .chars()
                .enumerate()
                .all(|(i, c)| {
                    if i == 0 {
                        c.is_ascii_lowercase()
                    } else {
                        c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'
                    }
                });
        if ok {
            Ok(())
        } else {
            Err(CompileError::BadName(name.into()))
        }
    }

    pub fn scaffold(&self, req: &ModuleScaffoldRequest) -> Result<ModuleScaffoldResponse, CompileError> {
        Self::validate_name(&req.name)?;
        let kind = if req.kind == "rust" { "rust" } else { "script" };
        let dest = if kind == "rust" {
            self.build_dir.join(&req.name)
        } else {
            self.src_dir.join(&req.name)
        };
        if dest.exists() {
            std::fs::remove_dir_all(&dest).map_err(|e| CompileError::Io(e.to_string()))?;
        }
        std::fs::create_dir_all(&dest).map_err(|e| CompileError::Io(e.to_string()))?;

        let tools = if req.tools.is_empty() {
            vec![ModuleTool {
                name: format!("{}.run", req.name),
                description: req.description.clone(),
                input_schema: serde_json::json!({"type":"object"}),
                output_schema: serde_json::json!({"type":"object"}),
            }]
        } else {
            req.tools.clone()
        };
        let caps = if req.required_caps.is_empty() {
            vec![format!("fs.write:/documents/{}/**", req.name)]
        } else {
            req.required_caps.clone()
        };

        if kind == "script" {
            let handlers = if req.source.trim().is_empty() {
                default_handlers_yaml(&req.name, &tools)
            } else {
                req.source.clone()
            };
            std::fs::write(dest.join("handlers.yaml"), handlers)
                .map_err(|e| CompileError::Io(e.to_string()))?;
            write_scaffold_ui(&dest, req, &tools)?;
            let manifest = ModuleManifest {
                name: req.name.clone(),
                version: "0.1.0".into(),
                hash: "pending".into(),
                permissions: ModulePermissions {
                    required_caps: caps,
                },
                tools,
                ui: Some(ModuleUi {
                    entry: "ui/index.html".into(),
                    mode: "declarative_ui".into(),
                }),
                min_os_api: 1,
            };
            let yaml = serde_yaml::to_string(&manifest).map_err(|e| CompileError::Io(e.to_string()))?;
            std::fs::write(dest.join("manifest.yaml"), yaml)
                .map_err(|e| CompileError::Io(e.to_string()))?;
        } else {
            let lib_rs = if req.source.trim().is_empty() {
                default_rust_lib(&req.name)
            } else {
                req.source.clone()
            };
            static_check_rust(&lib_rs)?;
            std::fs::create_dir_all(dest.join("src")).map_err(|e| CompileError::Io(e.to_string()))?;
            std::fs::write(dest.join("src/lib.rs"), lib_rs)
                .map_err(|e| CompileError::Io(e.to_string()))?;
            let cargo = format!(
                r#"[package]
name = "module-{name}"
version = "0.1.0"
edition = "2021"

[workspace]

[lib]
crate-type = ["cdylib"]

[dependencies]
aos-module-sdk = {{ path = "{sdk}" }}
serde = {{ version = "1", features = ["derive"] }}
serde_json = "1"
"#,
                name = req.name,
                sdk = self.sdk_path.display().to_string().replace('\\', "/")
            );
            std::fs::write(dest.join("Cargo.toml"), cargo)
                .map_err(|e| CompileError::Io(e.to_string()))?;
            write_scaffold_ui(&dest, req, &tools)?;
            let manifest = ModuleManifest {
                name: req.name.clone(),
                version: "0.1.0".into(),
                hash: "pending".into(),
                permissions: ModulePermissions {
                    required_caps: caps,
                },
                tools,
                ui: Some(ModuleUi {
                    entry: "ui/index.html".into(),
                    mode: "declarative_ui".into(),
                }),
                min_os_api: 1,
            };
            let yaml = serde_yaml::to_string(&manifest).map_err(|e| CompileError::Io(e.to_string()))?;
            std::fs::write(dest.join("manifest.yaml"), yaml)
                .map_err(|e| CompileError::Io(e.to_string()))?;
        }

        Ok(ModuleScaffoldResponse {
            path: dest.to_string_lossy().to_string(),
            kind: kind.into(),
        })
    }

    /// Package un module script avec le binaire ext-rt précompilé.
    pub fn package_script(&self, name: &str) -> Result<ModulePackageResponse, CompileError> {
        Self::validate_name(name)?;
        let src = self.src_dir.join(name);
        if !src.join("handlers.yaml").exists() {
            return Err(CompileError::Missing(format!(
                "{} (handlers.yaml)",
                src.display()
            )));
        }
        if !self.ext_rt_wasm.exists() {
            return Err(CompileError::Missing(format!(
                "ext-rt.wasm introuvable ({})",
                self.ext_rt_wasm.display()
            )));
        }
        let pkg = self.packages_dir.join(format!("{name}.aospkg"));
        if pkg.exists() {
            std::fs::remove_dir_all(&pkg).map_err(|e| CompileError::Io(e.to_string()))?;
        }
        std::fs::create_dir_all(pkg.join("ui")).map_err(|e| CompileError::Io(e.to_string()))?;
        std::fs::create_dir_all(pkg.join("assets")).map_err(|e| CompileError::Io(e.to_string()))?;

        std::fs::copy(&self.ext_rt_wasm, pkg.join("module.wasm"))
            .map_err(|e| CompileError::Io(e.to_string()))?;
        let handlers_raw = std::fs::read_to_string(src.join("handlers.yaml"))
            .map_err(|e| CompileError::Io(e.to_string()))?;
        // Convertir YAML → JSON pour le guest (pas de serde_yaml côté WASM).
        let handlers_val: serde_json::Value = serde_yaml::from_str(&handlers_raw)
            .map_err(|e| CompileError::Io(format!("handlers.yaml: {e}")))?;
        let handlers_json = serde_json::to_string_pretty(&handlers_val)
            .map_err(|e| CompileError::Io(e.to_string()))?;
        std::fs::write(pkg.join("handlers.yaml"), &handlers_raw)
            .map_err(|e| CompileError::Io(e.to_string()))?;
        std::fs::write(pkg.join("handlers.json"), &handlers_json)
            .map_err(|e| CompileError::Io(e.to_string()))?;
        std::fs::create_dir_all(pkg.join("assets")).map_err(|e| CompileError::Io(e.to_string()))?;
        std::fs::write(pkg.join("assets/handlers.json"), &handlers_json)
            .map_err(|e| CompileError::Io(e.to_string()))?;
        std::fs::copy(src.join("handlers.yaml"), pkg.join("assets/handlers.yaml"))
            .map_err(|e| CompileError::Io(e.to_string()))?;

        let wasm = std::fs::read(pkg.join("module.wasm")).map_err(|e| CompileError::Io(e.to_string()))?;
        let hash = sha256_hex(&wasm);

        let mut manifest: ModuleManifest = if src.join("manifest.yaml").exists() {
            let raw = std::fs::read_to_string(src.join("manifest.yaml"))
                .map_err(|e| CompileError::Io(e.to_string()))?;
            serde_yaml::from_str(&raw).map_err(|e| CompileError::Io(e.to_string()))?
        } else {
            ModuleManifest {
                name: name.into(),
                version: "0.1.0".into(),
                hash: String::new(),
                permissions: ModulePermissions {
                    required_caps: vec![format!("fs.write:/documents/{name}/**")],
                },
                tools: vec![ModuleTool {
                    name: format!("{name}.run"),
                    description: format!("Module script {name}"),
                    input_schema: serde_json::json!({"type":"object"}),
                    output_schema: serde_json::json!({"type":"object"}),
                }],
                ui: None,
                min_os_api: 1,
            }
        };
        manifest.name = name.into();
        manifest.hash = hash.clone();
        let yaml = serde_yaml::to_string(&manifest).map_err(|e| CompileError::Io(e.to_string()))?;
        std::fs::write(pkg.join("manifest.yaml"), yaml).map_err(|e| CompileError::Io(e.to_string()))?;
        let ui_doc = load_ui_for_package(&src, &manifest)?;
        std::fs::write(pkg.join("ui/index.html"), document_to_json(&ui_doc))
            .map_err(|e| CompileError::Io(e.to_string()))?;

        Ok(ModulePackageResponse {
            package_dir: pkg.to_string_lossy().to_string(),
            hash,
        })
    }

    /// Compile un crate Rust → wasm32 (offline) puis package.
    pub fn compile_rust(&self, name: &str) -> Result<ModuleCompileResponse, CompileError> {
        Self::validate_name(name)?;
        let crate_dir = self.build_dir.join(name);
        let lib = crate_dir.join("src/lib.rs");
        if !lib.exists() {
            return Err(CompileError::Missing(lib.display().to_string()));
        }
        let src = std::fs::read_to_string(&lib).map_err(|e| CompileError::Io(e.to_string()))?;
        static_check_rust(&src)?;

        // Vérifie rustc + target
        let rustc = Command::new("rustc").arg("--version").output();
        if rustc.as_ref().map(|o| !o.status.success()).unwrap_or(true) {
            return Err(CompileError::Toolchain(
                "rustc introuvable — utilisez module.package (ext-rt script) à la place".into(),
            ));
        }
        let target_ok = Command::new("rustc")
            .args(["--print", "target-list"])
            .output()
            .ok()
            .map(|o| {
                let s = String::from_utf8_lossy(&o.stdout);
                s.contains("wasm32-unknown-unknown")
            })
            .unwrap_or(false);
        if !target_ok {
            return Err(CompileError::Toolchain(
                "target wasm32-unknown-unknown absent — rustup target add wasm32-unknown-unknown"
                    .into(),
            ));
        }

        let output = Command::new("cargo")
            .arg("build")
            .arg("--manifest-path")
            .arg(crate_dir.join("Cargo.toml"))
            .arg("--target")
            .arg("wasm32-unknown-unknown")
            .arg("--release")
            .env("CARGO_NET_OFFLINE", "true")
            .current_dir(&crate_dir)
            .output()
            .map_err(|e| CompileError::Build(e.to_string()))?;

        let log = format!(
            "stdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        if !output.status.success() {
            return Err(CompileError::Build(log));
        }

        let wasm_name = format!("module_{}.wasm", name.replace('-', "_"));
        let wasm_candidates = [
            crate_dir
                .join("target/wasm32-unknown-unknown/release")
                .join(&wasm_name),
            crate_dir
                .join("target/wasm32-unknown-unknown/release")
                .join(format!("module-{name}.wasm")),
            crate_dir
                .join(format!("target/wasm32-unknown-unknown/release/module_{name}.wasm")),
        ];
        let wasm_src = wasm_candidates
            .iter()
            .find(|p| p.exists())
            .cloned()
            .or_else(|| {
                std::fs::read_dir(crate_dir.join("target/wasm32-unknown-unknown/release"))
                    .ok()?
                    .flatten()
                    .map(|e| e.path())
                    .find(|p| p.extension().and_then(|e| e.to_str()) == Some("wasm"))
            })
            .ok_or_else(|| CompileError::Build("binaire .wasm introuvable après build".into()))?;

        let pkg = self.packages_dir.join(format!("{name}.aospkg"));
        if pkg.exists() {
            std::fs::remove_dir_all(&pkg).map_err(|e| CompileError::Io(e.to_string()))?;
        }
        std::fs::create_dir_all(pkg.join("ui")).map_err(|e| CompileError::Io(e.to_string()))?;
        std::fs::copy(&wasm_src, pkg.join("module.wasm")).map_err(|e| CompileError::Io(e.to_string()))?;
        let wasm = std::fs::read(pkg.join("module.wasm")).map_err(|e| CompileError::Io(e.to_string()))?;
        let hash = sha256_hex(&wasm);

        let mut manifest: ModuleManifest = {
            let raw = std::fs::read_to_string(crate_dir.join("manifest.yaml"))
                .map_err(|e| CompileError::Io(e.to_string()))?;
            serde_yaml::from_str(&raw).map_err(|e| CompileError::Io(e.to_string()))?
        };
        manifest.hash = hash.clone();
        let yaml = serde_yaml::to_string(&manifest).map_err(|e| CompileError::Io(e.to_string()))?;
        std::fs::write(pkg.join("manifest.yaml"), yaml).map_err(|e| CompileError::Io(e.to_string()))?;
        let ui_doc = if crate_dir.join("ui/index.html").exists() {
            let raw = std::fs::read(crate_dir.join("ui/index.html"))
                .map_err(|e| CompileError::Io(e.to_string()))?;
            DeclUiDocument::parse_json(&raw)
                .map_err(|e| CompileError::Other(format!("ui: {e}")))?
        } else {
            ui_document_for_manifest(&manifest)
        };
        std::fs::write(pkg.join("ui/index.html"), document_to_json(&ui_doc))
            .map_err(|e| CompileError::Io(e.to_string()))?;

        Ok(ModuleCompileResponse {
            package_dir: pkg.to_string_lossy().to_string(),
            hash,
            log,
        })
    }
}

fn static_check_rust(src: &str) -> Result<(), CompileError> {
    for bad in FORBIDDEN_IN_SOURCE {
        if src.contains(bad) {
            return Err(CompileError::StaticCheck(format!(
                "motif interdit dans le source: {bad}"
            )));
        }
    }
    // Crates : seulement aos_module_sdk / serde / serde_json via use
    for line in src.lines() {
        let t = line.trim();
        if t.starts_with("extern crate") {
            return Err(CompileError::StaticCheck(
                "extern crate interdit".into(),
            ));
        }
        if let Some(rest) = t.strip_prefix("use ") {
            let crate_name = rest.split("::").next().unwrap_or("").trim();
            if !matches!(
                crate_name,
                "aos_module_sdk" | "serde" | "serde_json" | "std" | "core" | "alloc" | ""
            ) && !crate_name.is_empty()
                && !t.starts_with("use serde")
            {
                // allow serde::{...}
                if !["serde", "serde_json", "aos_module_sdk"].contains(&crate_name) {
                    // std is ok for String etc. but we already banned std::fs etc.
                    if crate_name != "std" && crate_name != "core" {
                        return Err(CompileError::StaticCheck(format!(
                            "crate non autorisé: {crate_name}"
                        )));
                    }
                }
            }
        }
    }
    Ok(())
}

fn default_handlers_yaml(name: &str, tools: &[ModuleTool]) -> String {
    let tool = tools
        .first()
        .map(|t| t.name.clone())
        .unwrap_or_else(|| format!("{name}.run"));
    format!(
        r#"tools:
  {tool}:
    steps:
      - service: fs.write
        args:
          path: "/documents/{name}/hello.md"
          content: "hello from {name}"
      - return:
          path: "/documents/{name}/hello.md"
"#
    )
}

fn default_rust_lib(name: &str) -> String {
    format!(
        r#"fn handle(tool: &str, args: &serde_json::Value) -> Result<serde_json::Value, String> {{
    match tool {{
        "{name}.run" => {{
            let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("hello");
            let path = format!("/documents/{name}/out.md");
            let version = aos_module_sdk::fs_write(&path, content)?;
            aos_module_sdk::json_ok(&serde_json::json!({{"path": path, "version": version}}))
        }}
        _ => Err(format!("outil inconnu: {{tool}}")),
    }}
}}
aos_module_sdk::export_module!(handle);
"#
    )
}

fn write_scaffold_ui(
    dest: &Path,
    req: &ModuleScaffoldRequest,
    tools: &[ModuleTool],
) -> Result<(), CompileError> {
    std::fs::create_dir_all(dest.join("ui")).map_err(|e| CompileError::Io(e.to_string()))?;
    let doc = if req.ui.trim().is_empty() {
        ui_document_for_tools(&req.name, &req.description, tools)
    } else {
        DeclUiDocument::parse_json(req.ui.as_bytes())
            .map_err(|e| CompileError::Other(format!("ui: {e}")))?
    };
    std::fs::write(dest.join("ui/index.html"), document_to_json(&doc))
        .map_err(|e| CompileError::Io(e.to_string()))
}

fn load_ui_for_package(src: &Path, manifest: &ModuleManifest) -> Result<DeclUiDocument, CompileError> {
    let ui_path = src.join("ui/index.html");
    if ui_path.exists() {
        let raw = std::fs::read(&ui_path).map_err(|e| CompileError::Io(e.to_string()))?;
        DeclUiDocument::parse_json(&raw).map_err(|e| CompileError::Other(format!("ui: {e}")))
    } else {
        Ok(ui_document_for_manifest(manifest))
    }
}

fn ui_document_for_manifest(manifest: &ModuleManifest) -> DeclUiDocument {
    ui_document_for_tools(&manifest.name, &manifest.name, &manifest.tools)
}

fn ui_document_for_tools(name: &str, description: &str, tools: &[ModuleTool]) -> DeclUiDocument {
    let primary = tools.first();
    let tool_name = primary
        .map(|t| t.name.as_str())
        .unwrap_or(name);
    let schema = primary
        .map(|t| t.input_schema.clone())
        .unwrap_or_else(|| serde_json::json!({"type":"object"}));
    let title = if description.is_empty() { name } else { description };
    default_document(title, tool_name, &schema)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

fn resolve_ext_rt_wasm(modules_dir: &Path) -> PathBuf {
    let candidates = [
        PathBuf::from("share/modules/ext-rt.aospkg/module.wasm"),
        modules_dir
            .parent()
            .unwrap_or(Path::new("."))
            .join("../share/modules/ext-rt.aospkg/module.wasm"),
        PathBuf::from("modules/ext-rt.aospkg/module.wasm"),
        std::env::var("AOS_HOME")
            .map(|h| PathBuf::from(h).join("share/modules/ext-rt.aospkg/module.wasm"))
            .unwrap_or_default(),
    ];
    for c in candidates {
        if c.exists() {
            return c;
        }
    }
    PathBuf::from("share/modules/ext-rt.aospkg/module.wasm")
}

fn resolve_sdk_path() -> PathBuf {
    let candidates = [
        PathBuf::from("modules/sdk"),
        PathBuf::from("../modules/sdk"),
        std::env::var("AOS_HOME")
            .map(|h| PathBuf::from(h).join("modules/sdk"))
            .unwrap_or_default(),
    ];
    for c in candidates {
        if c.exists() {
            return c.canonicalize().unwrap_or(c);
        }
    }
    PathBuf::from("modules/sdk")
}
