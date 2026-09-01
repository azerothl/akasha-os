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
            description: "Recherche web (auto: Brave→DDG→Bing)".into(),
            input_schema: serde_json::json!({
                "type":"object",
                "properties":{
                    "query":{"type":"string"},
                    "max_results":{"type":"integer"},
                    "engine":{"type":"string","description":"auto|brave|duckduckgo|bing"}
                },
                "required":["query"]
            }),
            backend: ToolBackend::Native,
            required_caps: vec!["net.connect:*".into()],
        },
        ToolDesc {
            name: "web.browse".into(),
            description: "Lire une page web (HTML→texte, sans JS)".into(),
            input_schema: serde_json::json!({
                "type":"object",
                "properties":{
                    "url":{"type":"string"},
                    "max_chars":{"type":"integer"}
                },
                "required":["url"]
            }),
            backend: ToolBackend::Native,
            required_caps: vec!["net.connect:*".into()],
        },
        ToolDesc {
            name: "net.fetch".into(),
            description: "Télécharger une URL vers le VFS (binaire)".into(),
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
        ToolDesc {
            name: "media.image.generate".into(),
            description: "Générer une image PNG locale (diffusion) sous /downloads ; options.init_image+/strength = img2img ; options.mask_image = inpaint (--mask)".into(),
            input_schema: serde_json::json!({
                "type":"object",
                "properties":{
                    "prompt":{"type":"string"},
                    "path":{"type":"string"},
                    "model_id":{"type":"string"},
                    "options":{
                        "type":"object",
                        "description":"closed sd.cpp options; init_image (logical /downloads path) + strength 0..1 for img2img; mask_image (logical /downloads path) for inpaint; unknown keys refused"
                    }
                },
                "required":["prompt"]
            }),
            backend: ToolBackend::Native,
            required_caps: vec!["media.generate".into(), "fs.write:/downloads/**".into()],
        },
        ToolDesc {
            name: "media.audio.generate".into(),
            description: "Synthèse vocale TTS locale (WAV) sous /downloads".into(),
            input_schema: serde_json::json!({
                "type":"object",
                "properties":{
                    "text":{"type":"string"},
                    "path":{"type":"string"},
                    "model_id":{"type":"string"},
                    "options":{"type":"object","description":"closed Piper options (unknown keys refused)"}
                },
                "required":["text"]
            }),
            backend: ToolBackend::Native,
            required_caps: vec!["media.generate".into(), "fs.write:/downloads/**".into()],
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
            description: "Déléguer à un sous-agent (brief COURT auto-suffisant ; tools/docs minimaux)"
                .into(),
            input_schema: serde_json::json!({
                "type":"object",
                "properties":{
                    "brief":{"type":"string","description":"≤3 phrases, auto-suffisant — pas de dump parent"},
                    "skills":{"type":"array","items":{"type":"string"}},
                    "tools":{"type":"array","items":{"type":"string"}},
                    "documents":{"type":"array"}
                },
                "required":["brief"]
            }),
            backend: ToolBackend::Runtime,
            required_caps: vec![],
        },
        ToolDesc {
            name: "agent.await".into(),
            description: "Attendre le résultat d'un sous-agent que tu as créé (child_id de agent.spawn)".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"child_id":{"type":"string"}},"required":["child_id"]}),
            backend: ToolBackend::Runtime,
            required_caps: vec![],
        },
        ToolDesc {
            name: "user.ask".into(),
            description: "Poser une question à l'utilisateur et attendre sa réponse (bloque jusqu'à la réponse)".into(),
            input_schema: serde_json::json!({
                "type":"object",
                "properties":{
                    "question":{"type":"string","description":"Question claire, une à la fois"},
                    "choices":{"type":"array","items":{"type":"string"},"description":"Options facultatives"}
                },
                "required":["question"]
            }),
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
            description: "Consulter la mémoire agent + utilisateur sur un sujet (à faire avant recherche externe)"
                .into(),
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
            input_schema: serde_json::json!({"type":"object","properties":{"name":{"type":"string"},"kind":{"type":"string"},"description":{"type":"string"},"source":{"type":"string"},"required_caps":{"type":"array"},"ui":{"type":"string","description":"Optional declarative_ui JSON (widgets: column, row, heading, text, markdown, stat_row, table, line_chart, bar_chart, form, button, select, radio, checkbox, textarea, image, audio)"}},"required":["name","description"]}),
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
            name: "module.uninstall".into(),
            description: "Désinstaller un module non bundlé (révoke tool.invoke, conserve /documents)".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"module":{"type":"string"}},"required":["module"]}),
            backend: ToolBackend::Native,
            required_caps: vec!["module.uninstall".into()],
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
    let notes_tools = [
        (
            "notes.create",
            "Créer une note (titre + content COURT / outline). Pour un long texte : create puis notes.update par sections",
            serde_json::json!({
                "type":"object",
                "properties":{
                    "title":{"type":"string"},
                    "content":{"type":"string","description":"Corps markdown (court à la création ; ≤ ~1200 car. recommandé)"}
                },
                "required":["title","content"]
            }),
        ),
        (
            "notes.update",
            "Mettre à jour une note (préférer sections incrémentales ≤ ~1200 car. de content)",
            serde_json::json!({
                "type":"object",
                "properties":{
                    "title":{"type":"string"},
                    "path":{"type":"string"},
                    "slug":{"type":"string"},
                    "content":{"type":"string","description":"Corps markdown complet ou section à écrire"},
                    "new_title":{"type":"string"}
                },
                "required":["content"]
            }),
        ),
        (
            "notes.list",
            "Lister les notes (titre, path, extrait)",
            serde_json::json!({"type":"object"}),
        ),
        (
            "notes.read",
            "Lire une note par title, path ou slug (inclut liens)",
            serde_json::json!({
                "type":"object",
                "properties":{
                    "title":{"type":"string"},
                    "path":{"type":"string"},
                    "slug":{"type":"string"}
                }
            }),
        ),
        (
            "notes.search",
            "Recherche sémantique dans les notes",
            serde_json::json!({
                "type":"object",
                "properties":{
                    "query":{"type":"string"},
                    "k":{"type":"integer"}
                },
                "required":["query"]
            }),
        ),
        (
            "notes.links",
            "Liens sortants et backlinks d'une note",
            serde_json::json!({
                "type":"object",
                "properties":{
                    "title":{"type":"string"},
                    "path":{"type":"string"},
                    "slug":{"type":"string"}
                }
            }),
        ),
        (
            "notes.related",
            "Notes liées (graphe) avec score de pertinence sur un sujet",
            serde_json::json!({
                "type":"object",
                "properties":{
                    "title":{"type":"string"},
                    "path":{"type":"string"},
                    "slug":{"type":"string"},
                    "topic":{"type":"string","description":"Sujet pour scorer la pertinence"},
                    "hops":{"type":"integer"},
                    "k":{"type":"integer"}
                }
            }),
        ),
    ];
    for (name, desc, schema) in notes_tools {
        v.push(ToolDesc {
            name: name.into(),
            description: desc.into(),
            input_schema: schema,
            backend: ToolBackend::Module,
            required_caps: vec!["tool.invoke:notes".into()],
        });
    }

    let tasks_tools = [
        (
            "tasks.create",
            "Créer une tâche partagée (humain + agent)",
            serde_json::json!({
                "type":"object",
                "properties":{
                    "title":{"type":"string"},
                    "notes":{"type":"string"}
                },
                "required":["title"]
            }),
        ),
        (
            "tasks.list",
            "Lister les tâches",
            serde_json::json!({"type":"object"}),
        ),
        (
            "tasks.update",
            "Mettre à jour une tâche",
            serde_json::json!({
                "type":"object",
                "properties":{
                    "id":{"type":"string"},
                    "title":{"type":"string"},
                    "notes":{"type":"string"},
                    "done":{"type":"boolean"}
                },
                "required":["id"]
            }),
        ),
        (
            "tasks.complete",
            "Marquer une tâche terminée (ou la réouvrir)",
            serde_json::json!({
                "type":"object",
                "properties":{
                    "id":{"type":"string"},
                    "done":{"type":"boolean"}
                },
                "required":["id"]
            }),
        ),
    ];
    for (name, desc, schema) in tasks_tools {
        v.push(ToolDesc {
            name: name.into(),
            description: desc.into(),
            input_schema: schema,
            backend: ToolBackend::Module,
            required_caps: vec!["tool.invoke:tasks".into()],
        });
    }

    let sid_schema = || {
        serde_json::json!({
            "type":"string",
            "description":"Omit — runtime binds the agent chat session_id (do not invent chat-1/default)"
        })
    };
    let canvas_tools = [
        (
            "canvas.set_style",
            "Définir le crayon de session (couleur #RRGGBB, épaisseur optionnelle) — les ops sans color/width héritent de ce style",
            serde_json::json!({
                "type":"object",
                "properties":{
                    "session_id": sid_schema(),
                    "color":{"type":"string","description":"#RRGGBB"},
                    "width":{"type":"number","description":"épaisseur relative 0..1"}
                }
            }),
        ),
        (
            "canvas.stroke",
            "Polyline sur le canvas de session (coords 0..1, max 1.0 — pas de pixels) — couleur/épaisseur optionnelles (héritent du crayon via canvas.set_style)",
            serde_json::json!({
                "type":"object",
                "properties":{
                    "session_id": sid_schema(),
                    "points":{"type":"array","items":{"type":"object","properties":{"x":{"type":"number"},"y":{"type":"number"}},"required":["x","y"]}},
                    "color":{"type":"string","description":"#RRGGBB"},
                    "width":{"type":"number","description":"épaisseur relative 0..1"}
                },
                "required":["points"]
            }),
        ),
        (
            "canvas.line",
            "Segment droit (2 points) sur le canvas de session (coords 0..1, max 1.0)",
            serde_json::json!({
                "type":"object",
                "properties":{
                    "session_id": sid_schema(),
                    "p0":{"type":"object","properties":{"x":{"type":"number"},"y":{"type":"number"}},"required":["x","y"]},
                    "p1":{"type":"object","properties":{"x":{"type":"number"},"y":{"type":"number"}},"required":["x","y"]},
                    "color":{"type":"string"},
                    "width":{"type":"number"}
                },
                "required":["p0","p1"]
            }),
        ),
        (
            "canvas.spline",
            "Courbe lisse (points de contrôle) sur le canvas de session (coords 0..1, max 1.0)",
            serde_json::json!({
                "type":"object",
                "properties":{
                    "session_id": sid_schema(),
                    "points":{"type":"array","items":{"type":"object","properties":{"x":{"type":"number"},"y":{"type":"number"}},"required":["x","y"]}},
                    "color":{"type":"string"},
                    "width":{"type":"number"}
                },
                "required":["points"]
            }),
        ),
        (
            "canvas.path",
            "Silhouette lisse : contour fermé rempli (fill:true par défaut) via points de contrôle — colline, corps, voiles. Préférer à empiler rect/spline.",
            serde_json::json!({
                "type":"object",
                "properties":{
                    "session_id": sid_schema(),
                    "points":{"type":"array","items":{"type":"object","properties":{"x":{"type":"number"},"y":{"type":"number"}},"required":["x","y"]}},
                    "color":{"type":"string"},
                    "width":{"type":"number","description":"épaisseur du contour ; 0 = remplissage seul"},
                    "fill":{"type":"boolean","description":"remplir la silhouette (défaut true)"},
                    "closed":{"type":"boolean","description":"fermer le contour (défaut true)"}
                },
                "required":["points"]
            }),
        ),
        (
            "canvas.rect",
            "Rectangle : x,y = coin haut-gauche, y vers le bas, w,h = taille (0..1). fill:true remplit. Alias cx,cy,w,h ; cx,cy,rx,ry ; width/height → w/h.",
            serde_json::json!({
                "type":"object",
                "properties":{
                    "session_id": sid_schema(),
                    "x":{"type":"number"},"y":{"type":"number"},
                    "w":{"type":"number"},"h":{"type":"number"},
                    "color":{"type":"string"},
                    "fill":{"type":"boolean"},
                    "width":{"type":"number"}
                },
                "required":["x","y","w","h"]
            }),
        ),
        (
            "canvas.ellipse",
            "Ellipse : x,y = coin haut-gauche, y vers le bas, w,h = taille (0..1). fill:true remplit. Alias cx,cy,w,h ; cx,cy,rx,ry ; width/height → w/h. Aligner : partager x et w.",
            serde_json::json!({
                "type":"object",
                "properties":{
                    "session_id": sid_schema(),
                    "x":{"type":"number"},"y":{"type":"number"},
                    "w":{"type":"number"},"h":{"type":"number"},
                    "color":{"type":"string"},
                    "fill":{"type":"boolean"},
                    "width":{"type":"number"}
                },
                "required":["x","y","w","h"]
            }),
        ),
        (
            "canvas.erase",
            "Effacer le long d'une polyline (peint le fond)",
            serde_json::json!({
                "type":"object",
                "properties":{
                    "session_id": sid_schema(),
                    "points":{"type":"array"},
                    "width":{"type":"number"}
                },
                "required":["points"]
            }),
        ),
        (
            "canvas.clear",
            "Effacer tout le canvas de session",
            serde_json::json!({
                "type":"object",
                "properties":{"session_id": sid_schema()}
            }),
        ),
        (
            "canvas.undo",
            "Annuler le dernier trait de l'auteur courant sur le canvas de session",
            serde_json::json!({
                "type":"object",
                "properties":{"session_id": sid_schema()}
            }),
        ),
        (
            "canvas.delete",
            "Supprimer l'objet canvas identifié par seq (voir digest canvas.get)",
            serde_json::json!({
                "type":"object",
                "properties":{
                    "session_id": sid_schema(),
                    "seq":{"type":"integer"}
                },
                "required":["seq"]
            }),
        ),
        (
            "canvas.move",
            "Déplacer l'objet seq de dx,dy en coords 0..1 (clamp)",
            serde_json::json!({
                "type":"object",
                "properties":{
                    "session_id": sid_schema(),
                    "seq":{"type":"integer"},
                    "dx":{"type":"number"},
                    "dy":{"type":"number"}
                },
                "required":["seq","dx","dy"]
            }),
        ),
        (
            "canvas.reorder",
            "Changer l'empilement de l'objet seq (z=0 arrière)",
            serde_json::json!({
                "type":"object",
                "properties":{
                    "session_id": sid_schema(),
                    "seq":{"type":"integer"},
                    "z":{"type":"integer"}
                },
                "required":["seq","z"]
            }),
        ),
        (
            "canvas.restyle",
            "Changer couleur / épaisseur / fill d'un objet seq",
            serde_json::json!({
                "type":"object",
                "properties":{
                    "session_id": sid_schema(),
                    "seq":{"type":"integer"},
                    "color":{"type":"string"},
                    "width":{"type":"number"},
                    "fill":{"type":"boolean"}
                },
                "required":["seq"]
            }),
        ),
        (
            "canvas.layer_create",
            "Créer un calque nommé (option parent_id pour un groupe)",
            serde_json::json!({
                "type":"object",
                "properties":{
                    "session_id": sid_schema(),
                    "name":{"type":"string"},
                    "parent_id":{"type":"string"}
                }
            }),
        ),
        (
            "canvas.layer_rename",
            "Renommer un calque",
            serde_json::json!({
                "type":"object",
                "properties":{
                    "session_id": sid_schema(),
                    "id":{"type":"string"},
                    "name":{"type":"string"}
                },
                "required":["id","name"]
            }),
        ),
        (
            "canvas.layer_set",
            "Hide / lock / opacity d'un calque (visible, locked, opacity 0..1)",
            serde_json::json!({
                "type":"object",
                "properties":{
                    "session_id": sid_schema(),
                    "id":{"type":"string"},
                    "visible":{"type":"boolean"},
                    "locked":{"type":"boolean"},
                    "opacity":{"type":"number"}
                },
                "required":["id"]
            }),
        ),
        (
            "canvas.layer_reorder",
            "Réordonner un calque (z parmi les frères ; parent_id optionnel)",
            serde_json::json!({
                "type":"object",
                "properties":{
                    "session_id": sid_schema(),
                    "id":{"type":"string"},
                    "parent_id":{"type":"string"},
                    "z":{"type":"integer"}
                },
                "required":["id","z"]
            }),
        ),
        (
            "canvas.layer_delete",
            "Supprimer un calque (ops réassignées au parent ou au défaut)",
            serde_json::json!({
                "type":"object",
                "properties":{
                    "session_id": sid_schema(),
                    "id":{"type":"string"}
                },
                "required":["id"]
            }),
        ),
        (
            "canvas.layer_activate",
            "Calque actif pour les prochains traits",
            serde_json::json!({
                "type":"object",
                "properties":{
                    "session_id": sid_schema(),
                    "id":{"type":"string"}
                },
                "required":["id"]
            }),
        ),
        (
            "canvas.align",
            "Aligner seq sur to_seq (ou la marge 0.10 si to_seq omis) : edges left|right|top|bottom|center_x|center_y",
            serde_json::json!({
                "type":"object",
                "properties":{
                    "session_id": sid_schema(),
                    "seq":{"type":"integer"},
                    "to_seq":{"type":"integer"},
                    "edges":{"type":"array","items":{"type":"string"}}
                },
                "required":["seq","edges"]
            }),
        ),
        (
            "canvas.rotate",
            "Rotation en degrés d'un rect/ellipse (pivot centre)",
            serde_json::json!({
                "type":"object",
                "properties":{
                    "session_id": sid_schema(),
                    "seq":{"type":"integer"},
                    "rotation":{"type":"number"}
                },
                "required":["seq","rotation"]
            }),
        ),
        (
            "canvas.get",
            "Lire le canvas existant (toujours en premier ; after_seq optionnel) — poursuis le dessin, ne redémarre pas sauf demande",
            serde_json::json!({
                "type":"object",
                "properties":{
                    "session_id": sid_schema(),
                    "after_seq":{"type":"integer"}
                }
            }),
        ),
        (
            "canvas.export",
            "Exporter le canvas (PNG par défaut ; format svg|json) sous /downloads",
            serde_json::json!({
                "type":"object",
                "properties":{
                    "session_id": sid_schema(),
                    "path":{"type":"string"},
                    "width":{"type":"integer"},
                    "height":{"type":"integer"},
                    "format":{"type":"string","description":"png (défaut), svg, ou json"}
                }
            }),
        ),
    ];
    for (name, desc, schema) in canvas_tools {
        v.push(ToolDesc {
            name: name.into(),
            description: desc.into(),
            input_schema: schema,
            backend: ToolBackend::Module,
            required_caps: vec!["tool.invoke:canvas".into()],
        });
    }
    v
}

/// Canvas tool ids (session vector drawing) — never part of `default_agent_tools`.
pub const CANVAS_TOOL_IDS: &[&str] = &[
    "canvas.set_style",
    "canvas.stroke",
    "canvas.line",
    "canvas.spline",
    "canvas.path",
    "canvas.rect",
    "canvas.ellipse",
    "canvas.erase",
    "canvas.clear",
    "canvas.undo",
    "canvas.get",
    "canvas.export",
    "canvas.delete",
    "canvas.move",
    "canvas.reorder",
    "canvas.restyle",
    "canvas.layer_create",
    "canvas.layer_rename",
    "canvas.layer_set",
    "canvas.layer_reorder",
    "canvas.layer_delete",
    "canvas.layer_activate",
    "canvas.align",
    "canvas.rotate",
];

/// Phrases that beat Create/image routing — must stay aligned with `chat_canvas` routing.
const EXPLICIT_CANVAS_MARKERS: &[&str] = &[
    "/canvas",
    "/canevas",
    "sur le canvas",
    "dans le canvas",
    "on the canvas",
    "in the canvas",
    "to the canvas",
    "sur le canevas",
    "dans le canevas",
    "on the canevas",
    "in the canevas",
    "to the canevas",
    "au trait",
];

/// Explicit vector-canvas intent (toggle phrase, slash, stroke wording).
pub fn explicit_canvas_intent(text: &str) -> bool {
    let lower = text.to_lowercase();
    EXPLICIT_CANVAS_MARKERS.iter().any(|m| lower.contains(m))
}

/// Append canvas tools when `include` is true (deduped).
/// `exported` = tool names from the installed `canvas` module (`module.list`);
/// only tools both in the catalog and exported are added.
pub fn merge_canvas_tools(
    tool_ids: &mut Vec<String>,
    include: bool,
    exported: &[String],
) {
    if !include {
        return;
    }
    for t in filter_canvas_tool_ids(exported) {
        if !tool_ids.iter().any(|x| x == &t) {
            tool_ids.push(t);
        }
    }
}

/// Intersect agent canvas catalog ids with tools the loaded canvas module exports.
pub fn filter_canvas_tool_ids(exported: &[String]) -> Vec<String> {
    CANVAS_TOOL_IDS
        .iter()
        .filter(|t| exported.iter().any(|e| e == *t))
        .map(|t| (*t).to_string())
        .collect()
}

/// Tool names exported by the installed `canvas` module (`module.list`).
pub fn canvas_tools_from_module_list(modules: &[aos_proto::ModuleInfo]) -> Vec<String> {
    modules
        .iter()
        .find(|m| m.name == "canvas")
        .map(|m| m.tools.clone())
        .unwrap_or_default()
}

/// Drop canvas.* tool ids that the loaded module does not export.
pub fn restrict_canvas_tools(tool_ids: &mut Vec<String>, exported: &[String]) {
    let allowed: std::collections::HashSet<String> = filter_canvas_tool_ids(exported)
        .into_iter()
        .collect();
    tool_ids.retain(|t| !t.starts_with("canvas.") || allowed.contains(t));
}

/// Short drawing strategy for canvas agents — only mentions exported tools.
pub fn canvas_draw_strategy_hint(exported: &[String]) -> String {
    if exported.iter().any(|t| t == "canvas.path") {
        "canvas.get puis canvas.path (silhouettes) ou canvas.stroke/rect/ellipse (fill:true pour remplir) en traits séquentiels.".into()
    } else {
        "canvas.get puis canvas.stroke/spline/rect/ellipse (fill:true pour remplir) en traits séquentiels.".into()
    }
}

/// Default Preview tool ids (UI agent create + scheduled fires).
pub fn default_agent_tools() -> Vec<String> {
    [
        "notes.create",
        "notes.list",
        "notes.read",
        "notes.search",
        "notes.update",
        "notes.links",
        "notes.related",
        "tasks.create",
        "tasks.list",
        "tasks.update",
        "tasks.complete",
        "fs.read",
        "fs.list",
        "fs.write",
        "web.search",
        "web.browse",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

/// Filtre le catalogue selon les ids sélectionnés (+ toujours les runtime de base).
pub fn select_tools(selected: &[String], extra: &[ToolDesc]) -> Vec<ToolDesc> {
    let catalog = builtin_catalog();
    let mut out: Vec<ToolDesc> = Vec::new();
    let always = [
        "plan.update",
        "goal.complete",
        "goal.fail",
        "user.ask",
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
        "module.uninstall",
        "module.list",
        "module.describe",
    ];
    for t in catalog.iter().chain(extra.iter()) {
        let keep = always.contains(&t.name.as_str())
            || selected.is_empty()
            || selected.iter().any(|s| s == &t.name || t.name.starts_with(&format!("{s}.")));
        // Si selected non vide : garder tools explicitement demandés + runtime always
        let keep = if selected.is_empty() {
            // Mode permissif : notes + tasks + runtime + fs + extensions
            matches!(t.backend, ToolBackend::Runtime)
                || t.name.starts_with("notes.")
                || t.name.starts_with("tasks.")
                || always.contains(&t.name.as_str())
                || t.name == "fs.read"
                || t.name == "fs.list"
                || t.name == "fs.write"
                || t.name == "mem.context"
                || t.name == "web.search"
                || t.name == "files.generate"
                || t.name == "media.image.generate"
                || t.name == "media.audio.generate"
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

/// Classe une action (backend outil + skill qui la déclare).
pub fn classify_action(
    name: &str,
    tools: &[ToolDesc],
    skills: &[(String, Vec<String>)],
) -> (String, Option<String>, Option<String>) {
    let canonical = canonicalize_tool_name(name);
    let name = canonical.as_str();
    let skill = skills
        .iter()
        .find(|(_, ts)| ts.iter().any(|t| t == name || name.starts_with(t)))
        .map(|(n, _)| n.clone());
    if let Some(t) = tools.iter().find(|t| t.name == name) {
        let (kind, mcp) = match &t.backend {
            ToolBackend::Native => ("native".to_string(), None),
            ToolBackend::Module => ("module".to_string(), None),
            ToolBackend::Mcp { server } => ("mcp".to_string(), Some(server.clone())),
            ToolBackend::Runtime => ("runtime".to_string(), None),
        };
        return (kind, mcp, skill);
    }
    if let Some(t) = builtin_catalog().iter().find(|t| t.name == name) {
        let (kind, mcp) = match &t.backend {
            ToolBackend::Native => ("native".to_string(), None),
            ToolBackend::Module => ("module".to_string(), None),
            ToolBackend::Mcp { server } => ("mcp".to_string(), Some(server.clone())),
            ToolBackend::Runtime => ("runtime".to_string(), None),
        };
        return (kind, mcp, skill);
    }
    // Nom de skill utilisé comme action (research, file.author, …)
    let skill_key = name.trim().to_ascii_lowercase().replace(['.', '_'], "-");
    if let Some((skill_name, _)) = skills
        .iter()
        .find(|(n, _)| n.trim().to_ascii_lowercase().replace(['.', '_'], "-") == skill_key)
    {
        return ("skill".into(), None, Some(skill_name.clone()));
    }
    if let Some(rest) = name.strip_prefix("mcp.") {
        let server = rest.split(':').next().map(|s| s.to_string());
        return ("mcp".into(), server, skill);
    }
    if is_module_fallback_candidate(name) {
        ("module".into(), None, skill)
    } else if name.contains('.') {
        ("native".into(), None, skill)
    } else {
        ("unknown".into(), None, skill)
    }
}

/// Noms d'outils hallucinés → catalogue natif (`media.audio.generate`, …).
pub fn canonicalize_tool_name(name: &str) -> String {
    let trimmed = name.trim();
    let stripped = trimmed
        .strip_prefix("tool.invoke:")
        .unwrap_or(trimmed)
        .trim();
    match stripped {
        "audio.generate" | "tts.generate" | "tts" | "speak" | "audio.tts" => {
            "media.audio.generate".into()
        }
        "image.generate" | "img.generate" | "image.gen" | "sd.generate" => {
            "media.image.generate".into()
        }
        other => other.to_string(),
    }
}

/// Prefixe qui n'est jamais un module WASM (évite `module inconnu: tool`).
pub fn reserved_tool_prefix(prefix: &str) -> bool {
    matches!(
        prefix,
        "fs" | "media"
            | "mem"
            | "web"
            | "net"
            | "files"
            | "cap"
            | "skill"
            | "module"
            | "agent"
            | "plan"
            | "user"
            | "memory"
            | "docs"
            | "goal"
            | "tool"
            | "audio"
            | "image"
            | "tts"
            | "mcp"
    )
}

/// Backend du catalogue filtré, sinon du catalogue builtin (outils natifs
/// absents du kit sélectionné, ex. `media.audio.generate`).
pub fn resolve_tool_backend(name: &str, tools: &[ToolDesc]) -> Option<ToolBackend> {
    if let Some(t) = tools.iter().find(|t| t.name == name) {
        return Some(t.backend.clone());
    }
    builtin_catalog()
        .into_iter()
        .find(|t| t.name == name)
        .map(|t| t.backend)
}

pub fn is_module_fallback_candidate(name: &str) -> bool {
    if !name.contains('.') || name.starts_with("mcp.") || name.starts_with("tool.invoke:") {
        return false;
    }
    let prefix = name.split('.').next().unwrap_or("");
    !reserved_tool_prefix(prefix)
}

/// Alias d'arguments LLM (`prompt` → `text` pour le TTS).
pub fn normalize_tool_args(name: &str, args: &serde_json::Value) -> serde_json::Value {
    let mut out = args.clone();
    let Some(obj) = out.as_object_mut() else {
        return out;
    };
    if name == "media.audio.generate" && !obj.contains_key("text") {
        for k in ["prompt", "content", "message", "speech"] {
            if let Some(v) = obj.get(k).cloned() {
                obj.insert("text".into(), v);
                break;
            }
        }
    }
    if name == "media.image.generate" && !obj.contains_key("prompt") {
        for k in ["text", "content", "description"] {
            if let Some(v) = obj.get(k).cloned() {
                obj.insert("prompt".into(), v);
                break;
            }
        }
    }
    if (name == "canvas.rect" || name == "canvas.ellipse")
        && !obj.contains_key("w")
        && !obj.contains_key("h")
        && obj.contains_key("width")
        && obj.contains_key("height")
    {
        if let Some(v) = obj.get("width").cloned() {
            obj.insert("w".into(), v);
        }
        if let Some(v) = obj.get("height").cloned() {
            obj.insert("h".into(), v);
        }
    }
    out
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
        assert!(t.iter().any(|x| x.name == "user.ask"));
        assert!(t.iter().any(|x| x.name == "notes.create"));
    }

    #[test]
    fn classify_notes_module_and_skill() {
        let tools = select_tools(&["notes.create".into()], &[]);
        let skills = vec![("notes-writer".into(), vec!["notes.create".into()])];
        let (kind, mcp, skill) = classify_action("notes.create", &tools, &skills);
        assert_eq!(kind, "module");
        assert!(mcp.is_none());
        assert_eq!(skill.as_deref(), Some("notes-writer"));
    }

    #[test]
    fn explicit_canvas_intent_markers() {
        assert!(explicit_canvas_intent("dessine sur le canvas"));
        assert!(explicit_canvas_intent("dessine dans le canvas"));
        assert!(explicit_canvas_intent("draw in the canvas"));
        assert!(explicit_canvas_intent("add to the canvas"));
        assert!(explicit_canvas_intent("dessine sur le canevas"));
        assert!(explicit_canvas_intent("/canvas"));
        assert!(!explicit_canvas_intent("dessine une maison"));
        assert!(!explicit_canvas_intent("canvas"));
        assert!(!explicit_canvas_intent("dessine sur canvas"));
    }

    #[test]
    fn merge_canvas_tools_adds_invoke_cap_targets() {
        let exported: Vec<String> = CANVAS_TOOL_IDS.iter().map(|s| (*s).to_string()).collect();
        let mut ids = vec!["notes.create".into()];
        merge_canvas_tools(&mut ids, true, &exported);
        assert!(ids.iter().any(|x| x == "canvas.stroke"));
        assert!(ids.iter().any(|x| x == "canvas.get"));
        let caps = caps_for_tools(&select_tools(&ids, &[]), &[]);
        assert!(caps.iter().any(|c| c == "tool.invoke:canvas"));
    }

    #[test]
    fn filter_canvas_tool_ids_intersects_exported() {
        let exported = vec![
            "canvas.stroke".into(),
            "canvas.rect".into(),
            "canvas.get".into(),
        ];
        let ids = filter_canvas_tool_ids(&exported);
        assert!(ids.iter().any(|x| x == "canvas.stroke"));
        assert!(ids.iter().any(|x| x == "canvas.rect"));
        assert!(!ids.iter().any(|x| x == "canvas.path"));
    }

    #[test]
    fn restrict_canvas_tools_drops_unexported_path() {
        let exported = vec!["canvas.stroke".into(), "canvas.rect".into()];
        let mut ids = vec![
            "canvas.path".into(),
            "canvas.stroke".into(),
            "notes.create".into(),
        ];
        restrict_canvas_tools(&mut ids, &exported);
        assert!(!ids.iter().any(|x| x == "canvas.path"));
        assert!(ids.iter().any(|x| x == "canvas.stroke"));
        assert!(ids.iter().any(|x| x == "notes.create"));
    }

    #[test]
    fn normalize_rect_width_height_aliases_to_w_h() {
        let args = normalize_tool_args(
            "canvas.rect",
            &serde_json::json!({
                "session_id": "s1",
                "x": 0.1,
                "y": 0.2,
                "width": 0.3,
                "height": 0.15,
                "fill": true
            }),
        );
        assert_eq!(args["w"], 0.3);
        assert_eq!(args["h"], 0.15);
    }

    #[test]
    fn default_agent_tools_grant_notes_tasks_fs_web() {
        let ids = default_agent_tools();
        let tools = select_tools(&ids, &[]);
        let caps = caps_for_tools(&tools, &[]);
        assert!(caps.iter().any(|c| c == "tool.invoke:notes"));
        assert!(caps.iter().any(|c| c == "tool.invoke:tasks"));
        assert!(!caps.iter().any(|c| c == "tool.invoke:canvas"));
        assert!(caps.iter().any(|c| c == "fs.read:**"));
        assert!(caps.iter().any(|c| c == "fs.write:**"));
        assert!(caps.iter().any(|c| c == "net.connect:*"));
        assert!(tools.iter().any(|t| t.name == "web.browse"));
        assert!(tools.iter().any(|t| t.name == "tasks.create"));
        assert!(!tools.iter().any(|t| t.name == "canvas.stroke"));
    }

    #[test]
    fn canvas_shape_tool_descriptions_name_bbox_contract() {
        let tools = select_tools(
            &["canvas.rect".into(), "canvas.ellipse".into()],
            &[],
        );
        let rect = tools.iter().find(|t| t.name == "canvas.rect").unwrap();
        let ellipse = tools.iter().find(|t| t.name == "canvas.ellipse").unwrap();
        assert!(rect.description.contains("coin haut-gauche"));
        assert!(rect.description.contains("cx,cy,w,h"));
        assert!(ellipse.description.contains("partager x et w"));
        assert!(ellipse.description.contains("fill:true"));
    }

    #[test]
    fn canonicalize_audio_aliases() {
        assert_eq!(
            canonicalize_tool_name("tool.invoke:audio.generate"),
            "media.audio.generate"
        );
        assert_eq!(canonicalize_tool_name("audio.generate"), "media.audio.generate");
        assert_eq!(canonicalize_tool_name("tts.generate"), "media.audio.generate");
        assert_eq!(canonicalize_tool_name("notes.create"), "notes.create");
        let args = normalize_tool_args(
            "media.audio.generate",
            &serde_json::json!({"prompt": "bonjour"}),
        );
        assert_eq!(args["text"], "bonjour");
        assert!(!is_module_fallback_candidate("audio.generate"));
        assert!(!is_module_fallback_candidate("tool.invoke:audio.generate"));
        assert!(is_module_fallback_candidate("notes.create"));
        let (kind, _, _) = classify_action("tool.invoke:audio.generate", &[], &[]);
        assert_eq!(kind, "native");
    }
}
