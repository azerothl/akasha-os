//! Catalogue d'outils et routage (natif / module / mcp / runtime).

use serde::{Deserialize, Serialize};

/// Backend d'exécution d'un outil.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolBackend {
    Native,
    Module,
    Mcp { server: String },
    Runtime,
}

/// Description d'outil injectée dans le prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDesc {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub backend: ToolBackend,
    pub required_caps: Vec<String>,
}

/// Catalogue de base des outils natifs plateforme + runtime.
pub fn builtin_catalog() -> Vec<ToolDesc> {
    let mut v = vec![
        ToolDesc {
            name: "fs.read".into(),
            description: "Lire un fichier du FS logique".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}),
            backend: ToolBackend::Native,
            required_caps: vec!["fs.read:**".into()],
        },
        ToolDesc {
            name: "fs.write".into(),
            description: "Écrire un fichier texte".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"}},"required":["path","content"]}),
            backend: ToolBackend::Native,
            required_caps: vec!["fs.write:**".into()],
        },
        ToolDesc {
            name: "fs.list".into(),
            description: "Lister des chemins sous un préfixe".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"prefix":{"type":"string"}}}),
            backend: ToolBackend::Native,
            required_caps: vec!["fs.read:**".into()],
        },
        ToolDesc {
            name: "mem.episodic_write".into(),
            description: "Écrire en mémoire épisodique".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"namespace":{"type":"string"},"text":{"type":"string"}},"required":["text"]}),
            backend: ToolBackend::Native,
            required_caps: vec![],
        },
        ToolDesc {
            name: "mem.episodic_query".into(),
            description: "Rechercher en mémoire épisodique".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"query":{"type":"string"},"k":{"type":"integer"}},"required":["query"]}),
            backend: ToolBackend::Native,
            required_caps: vec![],
        },
        ToolDesc {
            name: "mem.context".into(),
            description: "Construire un bloc de contexte RAG".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"query":{"type":"string"},"k":{"type":"integer"}},"required":["query"]}),
            backend: ToolBackend::Native,
            required_caps: vec![],
        },
        ToolDesc {
            name: "web.search".into(),
            description: "Recherche web".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"query":{"type":"string"},"max_results":{"type":"integer"}},"required":["query"]}),
            backend: ToolBackend::Native,
            required_caps: vec!["net.connect:*".into()],
        },
        ToolDesc {
            name: "net.fetch".into(),
            description: "Télécharger une URL".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"url":{"type":"string"}},"required":["url"]}),
            backend: ToolBackend::Native,
            required_caps: vec!["net.connect:*".into()],
        },
        ToolDesc {
            name: "files.generate".into(),
            description: "Générer un fichier (md/txt/json/csv/png/pdf)".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"path":{"type":"string"},"format":{"type":"string"},"content":{"type":"string"}},"required":["path","format"]}),
            backend: ToolBackend::Native,
            required_caps: vec!["fs.write:/documents/**".into()],
        },
        // Runtime
        ToolDesc {
            name: "plan.update".into(),
            description: "Mettre à jour le graphe de tâches".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"nodes":{"type":"array"}}}),
            backend: ToolBackend::Runtime,
            required_caps: vec![],
        },
        ToolDesc {
            name: "agent.spawn".into(),
            description: "Déléguer à un sous-agent".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"brief":{"type":"string"}},"required":["brief"]}),
            backend: ToolBackend::Runtime,
            required_caps: vec![],
        },
        ToolDesc {
            name: "agent.await".into(),
            description: "Attendre le résultat d'un sous-agent".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"child_id":{"type":"string"}},"required":["child_id"]}),
            backend: ToolBackend::Runtime,
            required_caps: vec![],
        },
        ToolDesc {
            name: "memory.remember".into(),
            description: "Mémoriser un fait pour cet agent".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"text":{"type":"string"}},"required":["text"]}),
            backend: ToolBackend::Runtime,
            required_caps: vec![],
        },
        ToolDesc {
            name: "memory.recall".into(),
            description: "Rappeler du contexte mémorisé".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"query":{"type":"string"}},"required":["query"]}),
            backend: ToolBackend::Runtime,
            required_caps: vec![],
        },
        ToolDesc {
            name: "docs.read".into(),
            description: "Lire un document attaché".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}),
            backend: ToolBackend::Runtime,
            required_caps: vec![],
        },
        ToolDesc {
            name: "goal.complete".into(),
            description: "Marquer le goal comme réussi".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"summary":{"type":"string"}}}),
            backend: ToolBackend::Runtime,
            required_caps: vec![],
        },
        ToolDesc {
            name: "goal.fail".into(),
            description: "Abandonner le goal".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"reason":{"type":"string"}}}),
            backend: ToolBackend::Runtime,
            required_caps: vec![],
        },
        // Extensions OS (F-EXT)
        ToolDesc {
            name: "cap.request".into(),
            description: "Demander une capacité manquante (trust + confirmation)".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"cap":{"type":"string"},"reason":{"type":"string"}},"required":["cap"]}),
            backend: ToolBackend::Native,
            required_caps: vec![],
        },
        ToolDesc {
            name: "skill.create".into(),
            description: "Créer une skill déclarative (recette markdown)".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"name":{"type":"string"},"description":{"type":"string"},"body":{"type":"string"},"tools":{"type":"array"},"required_caps":{"type":"array"}},"required":["name","description","body"]}),
            backend: ToolBackend::Native,
            required_caps: vec![],
        },
        ToolDesc {
            name: "skill.activate".into(),
            description: "Activer une skill (instructions + caps)".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}),
            backend: ToolBackend::Native,
            required_caps: vec![],
        },
        ToolDesc {
            name: "skill.list".into(),
            description: "Lister les skills disponibles".into(),
            input_schema: serde_json::json!({"type":"object"}),
            backend: ToolBackend::Native,
            required_caps: vec![],
        },
        ToolDesc {
            name: "module.scaffold".into(),
            description: "Scaffolder un module (script ou rust)".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"name":{"type":"string"},"kind":{"type":"string"},"description":{"type":"string"},"source":{"type":"string"},"required_caps":{"type":"array"}},"required":["name","description"]}),
            backend: ToolBackend::Native,
            required_caps: vec![],
        },
        ToolDesc {
            name: "module.package".into(),
            description: "Packager un module script avec ext-rt (sans rustc)".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}),
            backend: ToolBackend::Native,
            required_caps: vec![],
        },
        ToolDesc {
            name: "module.compile".into(),
            description: "Compiler un module Rust → WASM (critique, confirmation)".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}),
            backend: ToolBackend::Native,
            required_caps: vec!["module.compile".into()],
        },
        ToolDesc {
            name: "module.install".into(),
            description: "Installer un package .aospkg (critique)".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"source_dir":{"type":"string"},"approved_caps":{"type":"array"}},"required":["source_dir"]}),
            backend: ToolBackend::Native,
            required_caps: vec!["module.install".into()],
        },
        ToolDesc {
            name: "module.list".into(),
            description: "Lister les modules installés".into(),
            input_schema: serde_json::json!({"type":"object"}),
            backend: ToolBackend::Native,
            required_caps: vec![],
        },
        ToolDesc {
            name: "module.describe".into(),
            description: "Introspection manifeste + schémas d'un module".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"module":{"type":"string"}},"required":["module"]}),
            backend: ToolBackend::Native,
            required_caps: vec![],
        },
    ];

    // Module notes (toujours listé ; filtré par sélection)
    for (name, desc) in [
        ("notes.create", "Créer une note markdown"),
        ("notes.list", "Lister les notes"),
        ("notes.read", "Lire une note"),
        ("notes.search", "Chercher dans les notes"),
    ] {
        v.push(ToolDesc {
            name: name.into(),
            description: desc.into(),
            input_schema: serde_json::json!({"type":"object"}),
            backend: ToolBackend::Module,
            required_caps: vec!["tool.invoke:notes".into()],
        });
    }
    v
}

/// Filtre le catalogue selon les ids sélectionnés (+ toujours les runtime de base).
pub fn select_tools(selected: &[String], extra: &[ToolDesc]) -> Vec<ToolDesc> {
    let catalog = builtin_catalog();
    let mut out: Vec<ToolDesc> = Vec::new();
    let always = [
        "plan.update",
        "goal.complete",
        "goal.fail",
        "docs.read",
        "memory.remember",
        "memory.recall",
        "cap.request",
        "skill.create",
        "skill.activate",
        "skill.list",
        "module.scaffold",
        "module.package",
        "module.compile",
        "module.install",
        "module.list",
        "module.describe",
    ];
    for t in catalog.iter().chain(extra.iter()) {
        let keep = always.contains(&t.name.as_str())
            || selected.is_empty()
            || selected.iter().any(|s| s == &t.name || t.name.starts_with(&format!("{s}.")));
        // Si selected non vide : garder tools explicitement demandés + runtime always
        let keep = if selected.is_empty() {
            // Mode permissif : notes + runtime + fs + extensions
            matches!(t.backend, ToolBackend::Runtime)
                || t.name.starts_with("notes.")
                || always.contains(&t.name.as_str())
                || t.name == "fs.read"
                || t.name == "fs.list"
                || t.name == "fs.write"
                || t.name == "mem.context"
                || t.name == "web.search"
                || t.name == "files.generate"
        } else {
            keep || selected.iter().any(|s| &t.name == s) || always.contains(&t.name.as_str())
        };
        if keep && !out.iter().any(|x| x.name == t.name) {
            out.push(t.clone());
        }
    }
    // Toujours inclure agent.spawn/await si demandés ou en mode planner
    if selected.iter().any(|s| s == "agent.spawn" || s == "agent.await")
        || selected.is_empty()
    {
        for name in ["agent.spawn", "agent.await"] {
            if let Some(t) = catalog.iter().find(|t| t.name == name) {
                if !out.iter().any(|x| x.name == name) {
                    out.push(t.clone());
                }
            }
        }
    }
    out
}

/// Dérive les caps requises depuis les outils sélectionnés + MCP.
pub fn caps_for_tools(tools: &[ToolDesc], mcp_servers: &[String]) -> Vec<String> {
    let mut caps = Vec::new();
    for t in tools {
        for c in &t.required_caps {
            if !caps.contains(c) {
                caps.push(c.clone());
            }
        }
    }
    for s in mcp_servers {
        let c = format!("mcp.use:{s}");
        if !caps.contains(&c) {
            caps.push(c);
        }
    }
    caps
}

/// Vérifie que child_caps ⊆ parent_caps.
pub fn caps_subset(parent: &[String], child: &[String]) -> bool {
    child.iter().all(|c| {
        parent.iter().any(|p| {
            p == c
                || (p.ends_with(":*") && c.starts_with(p.trim_end_matches('*')))
                || (p.ends_with(":**") && c.starts_with(p.trim_end_matches("**")))
                || (p.contains("/**") && c.starts_with(p.split("/**").next().unwrap_or("")))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subset_ok() {
        let parent = vec!["tool.invoke:notes".into(), "fs.read:**".into()];
        let child = vec!["tool.invoke:notes".into()];
        assert!(caps_subset(&parent, &child));
        assert!(!caps_subset(&child, &parent));
    }

    #[test]
    fn select_includes_runtime() {
        let t = select_tools(&["notes.create".into()], &[]);
        assert!(t.iter().any(|x| x.name == "goal.complete"));
        assert!(t.iter().any(|x| x.name == "notes.create"));
    }
}
