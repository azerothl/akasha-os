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

/// Convertit le catalogue interne vers le format attendu par les chat
/// templates HuggingFace/OpenAI. Les templates natifs (Gemma 4 notamment)
/// utilisent cette structure pour injecter les déclarations `<|tool>`, alors
/// que le runtime continue d'exécuter le nom et les arguments normalisés.
pub fn chat_template_tool_definitions(tools: &[ToolDesc]) -> Vec<serde_json::Value> {
    tools
        .iter()
        .map(|tool| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.input_schema,
                }
            })
        })
        .collect()
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
            name: "mem.object.create".into(),
            description: "Créer un objet de mémoire cognitive typé".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"namespace":{"type":"string"},"kind":{"type":"string"},"title":{"type":"string"},"content":{"type":"string"},"status":{"type":"string"},"confidence":{"type":"number"},"importance":{"type":"number"},"temporal":{"type":"object"},"source_refs":{"type":"array"},"visibility":{"type":"string"},"metadata":{"type":"object"},"decision":{"type":"object"},"idempotency_key":{"type":"string"}},"required":["namespace","kind","content"]}),
            backend: ToolBackend::Native,
            required_caps: vec!["mem.write:*".into()],
        },
        ToolDesc {
            name: "mem.object.get".into(),
            description: "Lire un objet de mémoire cognitive".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"id":{"type":"integer"}},"required":["id"]}),
            backend: ToolBackend::Native,
            required_caps: vec!["mem.query:*".into()],
        },
        ToolDesc {
            name: "mem.decision.get".into(),
            description: "Lire une décision mémoire avec son statut".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"id":{"type":"integer"}},"required":["id"]}),
            backend: ToolBackend::Native,
            required_caps: vec!["mem.query:*".into()],
        },
        ToolDesc {
            name: "mem.object.list".into(),
            description: "Lister les objets de mémoire cognitive".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"namespace":{"type":"string"},"kind":{"type":"string"},"status":{"type":"string"},"limit":{"type":"integer"},"include_archived":{"type":"boolean"}}}),
            backend: ToolBackend::Native,
            required_caps: vec!["mem.query:*".into()],
        },
        ToolDesc {
            name: "mem.object.update".into(),
            description: "Mettre à jour un objet de mémoire cognitive".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"id":{"type":"integer"},"title":{"type":"string"},"content":{"type":"string"},"status":{"type":"string"},"confidence":{"type":"number"},"importance":{"type":"number"},"temporal":{"type":"object"},"source_refs":{"type":"array"},"visibility":{"type":"string"},"metadata":{"type":"object"},"decision":{"type":"object"}},"required":["id"]}),
            backend: ToolBackend::Native,
            required_caps: vec!["mem.write:*".into()],
        },
        ToolDesc {
            name: "mem.object.relate".into(),
            description: "Relier deux objets de mémoire cognitive".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"from":{"type":"integer"},"kind":{"type":"string"},"to":{"type":"integer"},"confidence":{"type":"number"},"source_refs":{"type":"array"}},"required":["from","kind","to"]}),
            backend: ToolBackend::Native,
            required_caps: vec!["mem.write:*".into()],
        },
        ToolDesc {
            name: "mem.graph.query".into(),
            description: "Explorer le graphe de mémoire".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"root_id":{"type":"integer"},"depth":{"type":"integer"},"max_nodes":{"type":"integer"},"relation":{"type":"string"}},"required":["root_id"]}),
            backend: ToolBackend::Native,
            required_caps: vec!["mem.query:*".into()],
        },
        ToolDesc {
            name: "mem.timeline".into(),
            description: "Lire la timeline temporelle de la mémoire".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"namespace":{"type":"string"},"subject_id":{"type":"integer"},"from_ms":{"type":"integer"},"to_ms":{"type":"integer"},"limit":{"type":"integer"}}}),
            backend: ToolBackend::Native,
            required_caps: vec!["mem.query:*".into()],
        },
        ToolDesc {
            name: "mem.explain".into(),
            description: "Expliquer un souvenir avec ses preuves".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"id":{"type":"integer"}},"required":["id"]}),
            backend: ToolBackend::Native,
            required_caps: vec!["mem.query:*".into()],
        },
        ToolDesc {
            name: "mem.revalidate".into(),
            description: "Revalider un objet mémoire et restaurer sa fraîcheur".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"id":{"type":"integer"},"confidence":{"type":"number"},"valid_to":{"type":"integer"},"status":{"type":"string"}},"required":["id"]}),
            backend: ToolBackend::Native,
            required_caps: vec!["mem.write:*".into()],
        },
        ToolDesc {
            name: "mem.narrative.generate".into(),
            description: "Générer une synthèse narrative de la mémoire".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"namespace":{"type":"string"},"from_ms":{"type":"integer"},"to_ms":{"type":"integer"},"title":{"type":"string"},"persist":{"type":"boolean"}}}),
            backend: ToolBackend::Native,
            required_caps: vec!["mem.query:*".into()],
        },
        ToolDesc {
            name: "mem.mind_palace.query".into(),
            description: "Naviguer dans les objets cognitifs par projet et relations".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"namespace":{"type":"string"},"root_id":{"type":"integer"},"limit":{"type":"integer"}}}),
            backend: ToolBackend::Native,
            required_caps: vec!["mem.query:*".into()],
        },
        ToolDesc {
            name: "mem.shadow.metrics".into(),
            description: "Lire les métriques de comparaison Memory V1/V2".into(),
            input_schema: serde_json::json!({"type":"object","properties":{}}),
            backend: ToolBackend::Native,
            required_caps: vec!["mem.query:*".into()],
        },
        ToolDesc {
            name: "mem.migration.status".into(),
            description: "Vérifier l'état de migration des projections Memory V2".into(),
            input_schema: serde_json::json!({"type":"object","properties":{}}),
            backend: ToolBackend::Native,
            required_caps: vec!["mem.query:*".into()],
        },
        ToolDesc {
            name: "web.search".into(),
            description: "Recherche web (auto: Brave→SearXNG→DDG→Bing)".into(),
            input_schema: serde_json::json!({
                "type":"object",
                "properties":{
                    "query":{"type":"string"},
                    "max_results":{"type":"integer"},
                    "engine":{"type":"string","description":"auto|brave|searxng|duckduckgo|bing"}
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
            required_caps: vec![
                "fs.write:/downloads/**".into(),
                "fs.write:/documents/**".into(),
            ],
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
        ToolDesc {
            name: "device.enumerate".into(),
            description: "Lister les caméras et microphones disponibles (Windows). Préalable à device.camera.capture / device.mic.capture.".into(),
            input_schema: serde_json::json!({"type":"object","properties":{}}),
            backend: ToolBackend::Native,
            required_caps: vec![],
        },
        ToolDesc {
            name: "system.hardware".into(),
            description: "Lire un snapshot à jour de la machine hôte (GPU, VRAM totale/utilisée/libre, RAM, disque, tier, thermique). À utiliser pour compatibilité modèle/GGUF ou contraintes locales — ne pas inventer meminfo ni lire des chemins hors sandbox.".into(),
            input_schema: serde_json::json!({"type":"object","properties":{}}),
            backend: ToolBackend::Native,
            required_caps: vec![],
        },
        ToolDesc {
            name: "device.camera.capture".into(),
            description: "Capturer une image PNG de la webcam. device_id optionnel (sinon première caméra). mode: once (défaut, une photo analysable) ou stream. Confirmation utilisateur requise. La PNG est jointe au tour vision suivant — décris ce que tu vois, n'invente pas que la webcam est indisponible.".into(),
            input_schema: serde_json::json!({
                "type":"object",
                "properties":{
                    "device_id":{"type":"string","description":"id renvoyé par device.enumerate ; omis = première caméra"},
                    "mode":{"type":"string","description":"once|stream"},
                    "max_duration_ms":{"type":"integer"}
                }
            }),
            backend: ToolBackend::Native,
            required_caps: vec!["device.camera.capture".into()],
        },
        ToolDesc {
            name: "device.mic.capture".into(),
            description: "Capturer un extrait microphone (Windows). device_id optionnel. Confirmation requise. La transcription vocale (STT) n'est pas dans Preview — n'affirme pas entendre le contenu.".into(),
            input_schema: serde_json::json!({
                "type":"object",
                "properties":{
                    "device_id":{"type":"string"},
                    "mode":{"type":"string","description":"once|stream"},
                    "max_duration_ms":{"type":"integer"}
                }
            }),
            backend: ToolBackend::Native,
            required_caps: vec!["device.mic.capture".into()],
        },
        ToolDesc {
            name: "device.capture.stop".into(),
            description: "Arrêter un flux device.camera.capture / device.mic.capture (mode stream). capture_id obligatoire.".into(),
            input_schema: serde_json::json!({
                "type":"object",
                "properties":{"capture_id":{"type":"string"}},
                "required":["capture_id"]
            }),
            backend: ToolBackend::Native,
            required_caps: vec![],
        },
        ToolDesc {
            name: "device.usb.enumerate".into(),
            description: "Lister les périphériques USB (ports série et génériques). Préalable à device.usb.open.".into(),
            input_schema: serde_json::json!({"type":"object","properties":{}}),
            backend: ToolBackend::Native,
            required_caps: vec![],
        },
        ToolDesc {
            name: "device.usb.open".into(),
            description: "Ouvrir un périphérique USB pour I/O (ports série USB sous Windows). Confirmation requise.".into(),
            input_schema: serde_json::json!({
                "type":"object",
                "properties":{
                    "device_id":{"type":"string","description":"id renvoyé par device.usb.enumerate"}
                },
                "required":["device_id"]
            }),
            backend: ToolBackend::Native,
            required_caps: vec!["device.usb.io".into()],
        },
        ToolDesc {
            name: "device.usb.read".into(),
            description: "Lire des octets depuis un handle USB ouvert (base64 dans la réponse).".into(),
            input_schema: serde_json::json!({
                "type":"object",
                "properties":{
                    "handle_id":{"type":"string"},
                    "max_bytes":{"type":"integer"},
                    "timeout_ms":{"type":"integer"}
                },
                "required":["handle_id"]
            }),
            backend: ToolBackend::Native,
            required_caps: vec!["device.usb.io".into()],
        },
        ToolDesc {
            name: "device.usb.write".into(),
            description: "Écrire des octets (base64) vers un handle USB ouvert.".into(),
            input_schema: serde_json::json!({
                "type":"object",
                "properties":{
                    "handle_id":{"type":"string"},
                    "data_base64":{"type":"string"},
                    "timeout_ms":{"type":"integer"}
                },
                "required":["handle_id","data_base64"]
            }),
            backend: ToolBackend::Native,
            required_caps: vec!["device.usb.io".into()],
        },
        ToolDesc {
            name: "device.usb.close".into(),
            description: "Fermer un handle USB ouvert.".into(),
            input_schema: serde_json::json!({
                "type":"object",
                "properties":{"handle_id":{"type":"string"}},
                "required":["handle_id"]
            }),
            backend: ToolBackend::Native,
            required_caps: vec!["device.usb.io".into()],
        },
        ToolDesc {
            name: "harness.run".into(),
            description: "Lancer un CLI coding allowlisté (codex, claude, grok) avec un prompt. Pas de shell libre ni d'argv extra. Confirmation requise. cwd optionnel (dossier existant).".into(),
            input_schema: serde_json::json!({
                "type":"object",
                "properties":{
                    "harness":{"type":"string","description":"codex | claude | grok"},
                    "prompt":{"type":"string","description":"consigne transmise au CLI (seul argument libre)"},
                    "cwd":{"type":"string","description":"dossier de travail existant (défaut AOS_HOME)"},
                    "timeout_sec":{"type":"integer","description":"15–600, défaut 180"}
                },
                "required":["harness","prompt"]
            }),
            backend: ToolBackend::Native,
            required_caps: vec!["harness.run".into()],
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
            name: "plan.create".into(),
            description: "Deep Thinking : créer un plan hiérarchique versionné (première action obligatoire en mode deep)".into(),
            input_schema: serde_json::json!({
                "type":"object",
                "properties":{
                    "title":{"type":"string"},
                    "task":{"type":"string"},
                    "steps":{"type":"array","description":"arbre d'étapes {id,label,description?,status?,children?}"}
                }
            }),
            backend: ToolBackend::Runtime,
            required_caps: vec![],
        },
        ToolDesc {
            name: "plan.update_step".into(),
            description: "Deep Thinking : patcher une étape (status, label, logs)".into(),
            input_schema: serde_json::json!({
                "type":"object",
                "properties":{
                    "plan_id":{"type":"string"},
                    "step_id":{"type":"string"},
                    "status":{"type":"string"},
                    "label":{"type":"string"},
                    "description":{"type":"string"},
                    "logs":{"type":"array","items":{"type":"string"}}
                },
                "required":["step_id"]
            }),
            backend: ToolBackend::Runtime,
            required_caps: vec![],
        },
        ToolDesc {
            name: "plan.replace_tree".into(),
            description: "Deep Thinking : réviser l'arbre du plan (ajout/suppression/réorg)".into(),
            input_schema: serde_json::json!({
                "type":"object",
                "properties":{
                    "plan_id":{"type":"string"},
                    "title":{"type":"string"},
                    "steps":{"type":"array"}
                },
                "required":["steps"]
            }),
            backend: ToolBackend::Runtime,
            required_caps: vec![],
        },
        ToolDesc {
            name: "plan.delegate_step".into(),
            description: "Deep Thinking : déléguer une étape via sous-agent (spawn + bind)".into(),
            input_schema: serde_json::json!({
                "type":"object",
                "properties":{
                    "plan_id":{"type":"string"},
                    "step_id":{"type":"string"},
                    "brief":{"type":"string"},
                    "skills":{"type":"array","items":{"type":"string"}},
                    "tools":{"type":"array","items":{"type":"string"}},
                    "documents":{"type":"array"}
                },
                "required":["step_id","brief"]
            }),
            backend: ToolBackend::Runtime,
            required_caps: vec![],
        },
        ToolDesc {
            name: "plan.get".into(),
            description: "Deep Thinking : lire le plan courant".into(),
            input_schema: serde_json::json!({
                "type":"object",
                "properties":{"plan_id":{"type":"string"}}
            }),
            backend: ToolBackend::Runtime,
            required_caps: vec![],
        },
        ToolDesc {
            name: "plan.append_log".into(),
            description: "Deep Thinking : ajouter un log interne à une étape".into(),
            input_schema: serde_json::json!({
                "type":"object",
                "properties":{
                    "plan_id":{"type":"string"},
                    "step_id":{"type":"string"},
                    "line":{"type":"string"}
                },
                "required":["step_id","line"]
            }),
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
            description: "Marquer le goal comme réussi. summary OBLIGATOIRE : résultat lisible pour l'utilisateur (liste, confirmation, chemins) — ne pas laisser vide.".into(),
            input_schema: serde_json::json!({
                "type":"object",
                "properties":{
                    "summary":{"type":"string","description":"Résultat final lisible pour l'utilisateur (obligatoire)"}
                },
                "required":["summary"]
            }),
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
                    "content":{"type":"string","description":"Corps markdown (court à la création ; ≤ ~1200 car. recommandé)"},
                    "tags":{"type":"array","items":{"type":"string"},"description":"Étiquettes de classement (ex. travail, idées)"}
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
                    "new_title":{"type":"string"},
                    "tags":{"type":"array","items":{"type":"string"},"description":"Remplace les étiquettes si fourni"}
                },
                "required":["content"]
            }),
        ),
        (
            "notes.list",
            "Lister les notes (titre, path, extrait, étiquettes)",
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
        (
            "notes.delete",
            "Supprimer une note (fichier + mémoire + graphe)",
            serde_json::json!({
                "type":"object",
                "properties":{
                    "title":{"type":"string"},
                    "path":{"type":"string"},
                    "slug":{"type":"string"}
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

    let sid_schema = || {
        serde_json::json!({
            "type":"string",
            "description":"Omit — runtime binds the agent chat session_id (do not invent chat-1/default)"
        })
    };
    let scene_geometry_schema = serde_json::json!({
        "oneOf":[
            {"type":"object","properties":{
                "kind":{"const":"rect"},"x":{"type":"number"},"y":{"type":"number"},
                "w":{"type":"number","exclusiveMinimum":0},"h":{"type":"number","exclusiveMinimum":0},
                "rotation":{"type":"number"}
            },"required":["kind","x","y","w","h"]},
            {"type":"object","properties":{
                "kind":{"const":"ellipse"},"x":{"type":"number"},"y":{"type":"number"},
                "w":{"type":"number","exclusiveMinimum":0},"h":{"type":"number","exclusiveMinimum":0},
                "rotation":{"type":"number"}
            },"required":["kind","x","y","w","h"]},
            {"type":"object","properties":{
                "kind":{"const":"line"},"p0":{"type":"object","properties":{"x":{"type":"number"},"y":{"type":"number"}},"required":["x","y"]},
                "p1":{"type":"object","properties":{"x":{"type":"number"},"y":{"type":"number"}},"required":["x","y"]}
            },"required":["kind","p0","p1"]},
            {"type":"object","properties":{
                "kind":{"enum":["path","spline"]},
                "points":{"type":"array","minItems":2,"items":{"type":"object","properties":{"x":{"type":"number"},"y":{"type":"number"}},"required":["x","y"]}},
                "closed":{"type":"boolean"}
            },"required":["kind","points"]},
            {"type":"object","properties":{
                "kind":{"const":"text"},"x":{"type":"number"},"y":{"type":"number"},"text":{"type":"string"},
                "size":{"type":"number","exclusiveMinimum":0},"rotation":{"type":"number"}
            },"required":["kind","x","y","text"]}
        ]
    });
    let scene_schema = serde_json::json!({
        "type":"object",
        "description":"CanvasSceneSpec v1. Coordonnées normalisées 0..1, origine en haut à gauche. Pour rect/ellipse, x,y est le coin haut-gauche et w,h la taille : le centre vaut (x+w/2,y+h/2), donc centre=.5 avec w=.4 signifie x=.3, pas x=.5.",
        "properties":{
            "version":{"const":1},
            "profile":{"enum":["primitives","illustration","diagram","math","freeform"]},
            "subject":{"type":"string"},"reference":{"type":"string"},"view":{"type":"string"},
            "elements":{"type":"array","minItems":1,"items":{"type":"object","properties":{
                "id":{"type":"string"},"role":{"type":"string"},"layer":{"type":"string"},
                "color":{"type":"string","description":"#RRGGBB"},"width":{"type":"number"},"fill":{"type":"boolean"},
                "opacity":{"type":"number"},"dash":{"type":"array","items":{"type":"number"}},
                "geometry":scene_geometry_schema
            },"required":["id","geometry"]}},
            "relations":{"type":"array","items":{"type":"object","properties":{
                "from":{"type":"string"},"relation":{"type":"string","description":"ex. attached_to, overlaps, aligns_with"},"to":{"type":"string"}
            },"required":["from","relation","to"]}},
            "guides":{"type":"object"}
        },
        "required":["elements"]
    });
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
                    "color":{"type":"string","description":"#RRGGBB (alias fill_color)"},
                    "fill_color":{"type":"string","description":"alias de color"},
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
                    "color":{"type":"string","description":"#RRGGBB (alias fill_color)"},
                    "fill_color":{"type":"string","description":"alias de color"},
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
                    "color":{"type":"string","description":"#RRGGBB (alias fill_color)"},
                    "fill_color":{"type":"string","description":"alias de color"},
                    "width":{"type":"number"}
                },
                "required":["points"]
            }),
        ),
        (
            "canvas.path",
            "Silhouette lisse : contour fermé rempli (fill:true par défaut) via points de contrôle. Préférer à empiler rect/spline. Passe color (#RRGGBB) sur chaque op.",
            serde_json::json!({
                "type":"object",
                "properties":{
                    "session_id": sid_schema(),
                    "points":{"type":"array","items":{"type":"object","properties":{"x":{"type":"number"},"y":{"type":"number"}},"required":["x","y"]}},
                    "color":{"type":"string","description":"#RRGGBB (alias fill_color)"},
                    "fill_color":{"type":"string","description":"alias de color pour le remplissage"},
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
                    "color":{"type":"string","description":"#RRGGBB (alias fill_color)"},
                    "fill_color":{"type":"string","description":"alias de color"},
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
                    "color":{"type":"string","description":"#RRGGBB (alias fill_color)"},
                    "fill_color":{"type":"string","description":"alias de color"},
                    "fill":{"type":"boolean"},
                    "width":{"type":"number"}
                },
                "required":["x","y","w","h"]
            }),
        ),
        (
            "canvas.text",
            "Étiquette texte ancrée en (x,y), coin haut-gauche (coords 0..1, max 1.0). size = hauteur de ligne relative (défaut 0.05). 500 caractères max. rotation en degrés (pivot à l'ancre).",
            serde_json::json!({
                "type":"object",
                "properties":{
                    "session_id": sid_schema(),
                    "x":{"type":"number"},"y":{"type":"number"},
                    "text":{"type":"string"},
                    "size":{"type":"number","description":"hauteur de ligne relative 0..1 (défaut 0.05)"},
                    "color":{"type":"string","description":"#RRGGBB (alias fill_color)"},
                    "fill_color":{"type":"string","description":"alias de color"},
                    "rotation":{"type":"number","description":"degrés, pivot à l'ancre"},
                    "opacity":{"type":"number"}
                },
                "required":["x","y","text"]
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
            "canvas.set_guides",
            "Configurer les guides partagés (grille visible, aimant, taille de grille et mode grid|anchors|edges|grid_and_anchors)",
            serde_json::json!({
                "type":"object",
                "properties":{
                    "session_id": sid_schema(),
                    "show_grid":{"type":"boolean"},
                    "snap":{"type":"boolean"},
                    "grid_size":{"type":"number","description":"pas normalisé 0.001..0.25"},
                    "snap_mode":{"type":"string","description":"grid|anchors|edges|grid_and_anchors"}
                }
            }),
        ),
        (
            "canvas.compose",
            "Compiler une CanvasSceneSpec versionnée en une scène complète. Coordonnées 0..1, origine haut-gauche : pour rect/ellipse x,y=coin haut-gauche et w,h=taille (centre=(x+w/2,y+h/2), jamais x,y=centre). Pour profile=illustration, les masses qui composent le sujet doivent se chevaucher ou se toucher ; indique les relations attached_to/overlaps et vérifie les avertissements scene_check avant export. Pour un graphe ou des maths, préférer les primitives.",
            serde_json::json!({
                "type":"object",
                "properties":{
                    "session_id": sid_schema(),
                    "author_id":{"type":"string"},
                    "scene":scene_schema
                },
                "required":["scene"]
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
    v.extend(illust_tool_descs());
    v
}

/// Illustration surface tools (native platform intents).
pub fn illust_tool_descs() -> Vec<ToolDesc> {
    let sid = || {
        serde_json::json!({"type":"string","description":"omis — le runtime force session_id"})
    };
    let tools = [
        (
            "illust.get",
            "Lire brief/spec/digest Illustration (toujours en premier)",
            serde_json::json!({"type":"object","properties":{"session_id":sid()}}),
        ),
        (
            "illust.set_brief",
            "Fixer subject/look/palette/anchor avant compose. look: ink|riso|screen|pencil|blueprint|doodle. engine: laisser flat. sand, paper ou found seulement si l'utilisateur demande du sable, un livre pop-up, ou une vidéo. Ne jamais mettre paper pour un chat ou un dessin.",
            serde_json::json!({
                "type":"object",
                "properties":{
                    "session_id":sid(),
                    "brief":{
                        "type":"object",
                        "properties":{
                            "subject":{"type":"string"},
                            "look":{"type":"string","enum":["ink","riso","screen","pencil","blueprint","doodle"]},
                            "palette":{"type":"string","enum":["paperInk","risoPop","screenSea","pencilMinimal","blueprintNight"]},
                            "anchor":{"type":"string"},
                            "beats":{"type":"array","items":{"type":"string"}},
                            "engine":{"type":"string","enum":["flat","sand","paper","found"]},
                            "photo":{"type":"string","description":"chemin local pour le look doodle"},
                            "video":{"type":"string","description":"refusé: pas de traceur rotoscope"}
                        },
                        "required":["subject"]
                    },
                    "subject":{"type":"string","description":"alias flat — préférer brief.subject"},
                    "look":{"type":"string"},
                    "palette":{"type":"string"},
                    "anchor":{"type":"string"}
                },
                "required":["brief"]
            }),
        ),
        (
            "illust.resolve_image",
            "Après comparaison visuelle de image_run.source_png et image_run.candidate_png, choisir la retouche (keep_candidate=true) ou conserver l'original (false). Fournir l'id courant image_run.id comme run_id. Obligatoire avant export ou nouvelle retouche. Cette sélection ne constitue pas une validation artistique.",
            serde_json::json!({"type":"object","properties":{
                "session_id":sid(),"run_id":{"type":"string"},"keep_candidate":{"type":"boolean"}},"required":["run_id","keep_candidate"]}),
        ),
        (
            "illust.refine_image",
            "Retouche le dernier rendu image terminé. Après inspection visuelle, décris une correction précise dans correction (anatomie, contact, repères résiduels, détail manquant). Conserve le sujet et la composition. Exécution en arrière-plan; illust.get expose le nouveau résultat, l'historique et l'état. Ne pas appeler pendant running. needs_review ne vaut pas approbation artistique.",
            serde_json::json!({"type":"object","properties":{
                "session_id":sid(),"correction":{"type":"string"}},"required":["correction"]}),
        ),
        (
            "illust.generate_image",
            "Lance les passes de dessin par moteur d'édition local en arrière-plan. Après set_brief, fournis construction : description concise de la pose, de l'espèce, du cadrage, des appuis et des relations spatiales; pas de vêtements, visage, texture ou couleur à ce stade. Chaque passe reprend l'image précédente et devient visible dans Illustration. Consulte illust.get pour suivre image_run; ne relance pas tant que running. needs_review signifie terminé mais NON validé visuellement. Aucun fallback en formes procédurales si le moteur est absent.",
            serde_json::json!({"type":"object","properties":{
                "session_id":sid(),"seed":{"type":"integer","minimum":0,"maximum":u32::MAX,"description":"Optionnel : graine à reproduire. Omettre pour une nouvelle variante; image_run.seed conserve la valeur utilisée. Une nouvelle variante ne garantit pas une amélioration."},
                "pose_reference_png":{"type":"string","description":"Optionnel : chemin existant d'un guide de pose PNG dans /downloads Akasha, maximum 2048x2048 et 16 Mio. Inspecter le guide et vérifier son adéquation au sujet avant utilisation. Guide la première passe seulement; ce n'est pas une garantie anatomique. Ne jamais inventer un chemin ni fournir un chemin du système hôte."},
                "construction":{"type":"string","description":"Pose, espèce, masses et relations spatiales de l'image-clé. Préciser les rapports de taille plausibles entre sujets et mobilier, les appuis, les occlusions et la surface de contact exacte. Pour un mouvement vers un support, décrire la phase du geste, la direction du centre de masse et la proximité des points de contact avec la surface cible; éviter une pose suspendue sans interaction lisible. Reporter ces contraintes dans frame_subject."},
                "frame_subject":{"type":"string","description":"Description complète de l'unique image-clé représentée : sujets et leur nombre, détails distinctifs, style et une seule action. Pour une demande séquentielle, omettre les actions précédentes/suivantes plutôt que demander de les ignorer. La construction doit représenter ce même instant. La demande originale reste intacte dans brief.subject; cette image ne vaut pas animation complète."}},"required":["construction","frame_subject"]}),
        ),
        (
            "illust.compose",
            "Publie une passe réelle de dessin et son aperçu last_png. Utilise construction_phase dans l'ordre skeleton, volumes, contours, details, final. Chaque spec remplace la précédente : conserve les données des passes déjà construites. Pose et proportions dans skeleton, masses dans volumes, contours visibles dans contours, habillage dans details. Pour final, assemble explicitement parts avec les chevauchements corrects. Coordonnées normalisées 0..1 ; x/y est le coin haut-gauche pour ellipse/rect. Maintiens contacts, proportions, orientation et identité entre les passes. Une construction explicite est préservée par le moteur. Après le rendu final : render_sheet puis review/export. Le score de review est structurel, pas une mesure de qualité artistique.",
            serde_json::json!({
                "type":"object",
                "properties":{
                    "session_id":sid(),
                    "spec":{
                        "type":"object",
                        "properties":{
                            "brief":{"type":"object"},
                            "parts":{
                                "type":"array",
                                "items":{
                                    "type":"object",
                                    "properties":{
                                        "id":{"type":"string"},
                                        "role":{"type":"string"},
                                        "fill":{"type":"boolean"},
                                        "outline":{"type":"boolean"},
                                        "fill_index":{"type":"integer"},
                                        "seed":{"type":"integer"},
                                        "geometry":{
                                            "type":"object",
                                            "properties":{
                                                "kind":{"type":"string","enum":["ellipse","rect","path"]},
                                                "x":{"type":"number"},
                                                "y":{"type":"number"},
                                                "w":{"type":"number"},
                                                "h":{"type":"number"},
                                                "points":{
                                                    "type":"array",
                                                    "minItems":8,
                                                    "items":{
                                                        "type":"object",
                                                        "properties":{"x":{"type":"number"},"y":{"type":"number"}},
                                                        "required":["x","y"]
                                                    }
                                                }
                                            },
                                            "required":["kind"]
                                        }
                                    },
                                    "required":["id","role","geometry"]
                                }
                            },
                            "key_drawings":{
                                "type":"array",
                                "description":"Dessins complets de remplacement, un par beat nommé; chaque dessin contient tous ses parts et conserve ses chevauchements.",
                                "items":{
                                    "type":"object",
                                    "properties":{
                                        "id":{"type":"string"},
                                        "pose":{"type":"object"},
                                        "parts":{"type":"array"}
                                    },
                                    "required":["id","parts"]
                                }
                            },
                            "construction":{
                                "type":"object",
                                "description":"Plan multi-passes : ligne d'action, point focal, valeurs, orientation, silhouette et model sheet.",
                                "properties":{
                                    "archetype":{"type":"string"},
                                    "passes":{"type":"array","items":{"type":"string"}},
                                    "must_read":{"type":"array","items":{"type":"string"}},
                                    "relations":{"type":"array","items":{"type":"string"}},
                                    "action_line":{"type":"string"},
                                    "focal_point":{"type":"string"},
                                    "value_groups":{"type":"array","items":{"type":"string"}},
                                    "head_orientation":{"type":"string"},
                                    "silhouette_test":{"type":"boolean"},
                                    "model_sheet":{"type":"array","items":{"type":"string"}},
                                    "material_pass":{"type":"string"}
                                }
                            },
                            "construction_phase":{"type":"string","enum":["skeleton","volumes","contours","details","final"]},
                            "skeleton":{"type":"array","description":"Articulations et relations parent/enfant de la pose.","items":{"type":"object"}},
                            "contacts":{"type":"object","description":"Contacts nommés : [racine, articulation intermédiaire, extrémité, cible]. Exemple prise_pipe:[epaule,coude,main,prise_pipe]. Ajouter la cible au skeleton. Le moteur conserve les longueurs, ajuste le coude et place la main sur la cible avant de résoudre joint_bindings. Racines et cibles fixes, chaînes indépendantes ; une cible hors de portée produit une erreur à corriger.","additionalProperties":{"type":"array","items":{"type":"string"},"minItems":4,"maxItems":4}},
                            "joint_bindings":{"type":"object","description":"Associe un id de path ou volume à [articulation origine, articulation axe]. Coordonnées locales : (0,0)=origine, (1,0)=axe, y perpendiculaire en longueurs du segment. Applicable à parts, contours, details et volumes (x/y/w/h locaux, rotation relative en radians). Attacher masses, manches et détails aux mêmes articulations pour suivre la pose.","additionalProperties":{"type":"array","items":{"type":"string"},"minItems":2,"maxItems":2}},
                            "volumes":{"type":"array","description":"Volumes simples avant contour.","items":{"type":"object"}},
                            "contours":{"type":"array","description":"Contours principaux issus des volumes.","items":{"type":"object"}},
                            "details":{"type":"array","description":"Détails identitaires appliqués après la silhouette.","items":{"type":"object"}}
                        },
                        "required":["parts"]
                    }
                },
                "required":["spec"]
            }),
        ),
        (
            "illust.render_sheet",
            "Rendre la planche style (sujet @0.6/1/1.8 + swatches) et la model sheet (face/profil/3-4/dos/expression/pose) → PNG sous /downloads/illustration",
            serde_json::json!({"type":"object","properties":{"session_id":sid(),"width":{"type":"integer"}}}),
        ),
        (
            "illust.review",
            "Checklist déterministe (brief, parts, anchor, silhouette)",
            serde_json::json!({"type":"object","properties":{"session_id":sid()}}),
        ),
        (
            "illust.export",
            "Exporter le still PNG (ou json) sous /downloads/illustration et libérer le lock agent",
            serde_json::json!({
                "type":"object",
                "properties":{
                    "session_id":sid(),
                    "path":{"type":"string"},
                    "width":{"type":"integer"},
                    "height":{"type":"integer"},
                    "format":{"type":"string"}
                }
            }),
        ),
        (
            "illust.animate",
            "Animer la scène (timeline optionnelle) → dessins à 24fps + mp4 24fps si ffmpeg",
            serde_json::json!({
                "type":"object",
                "properties":{
                    "session_id":sid(),
                    "timeline":{"type":"object"},
                    "width":{"type":"integer"},
                    "path":{"type":"string"}
                }
            }),
        ),
    ];
    tools
        .into_iter()
        .map(|(name, desc, schema)| ToolDesc {
            name: name.into(),
            description: desc.into(),
            input_schema: schema,
            backend: ToolBackend::Native,
            required_caps: vec!["tool.invoke:illust".into()],
        })
        .collect()
}

pub const ILLUST_TOOL_IDS: &[&str] = &[
    "illust.get",
    "illust.set_brief",
    "illust.compose",
    "illust.generate_image",
    "illust.refine_image",
    "illust.resolve_image",
    "illust.render_sheet",
    "illust.review",
    "illust.export",
    "illust.animate",
];

pub fn merge_illust_tools(tool_ids: &mut Vec<String>, include: bool) {
    if !include {
        return;
    }
    for t in ILLUST_TOOL_IDS {
        if !tool_ids.iter().any(|x| x == t) {
            tool_ids.push((*t).to_string());
        }
    }
}

pub fn agent_has_illust_tools(tool_ids: &[String]) -> bool {
    tool_ids.iter().any(|t| t.starts_with("illust."))
}

/// Illustration agents must not inherit spawn / user.ask from select_tools "always".
pub fn strip_illust_blocked_runtime_tools(tools: &mut Vec<ToolDesc>, spec_tool_ids: &[String]) {
    if !agent_has_illust_tools(spec_tool_ids) {
        return;
    }
    for blocked in ["user.ask", "agent.spawn", "agent.await"] {
        if spec_tool_ids.iter().any(|t| t == blocked) {
            continue;
        }
        tools.retain(|t| t.name != blocked);
    }
}

pub fn explicit_illust_intent(text: &str) -> bool {
    let lower = text.to_lowercase();
    const MARKERS: &[&str] = &[
        "/illust",
        "/illustration",
        "illustre",
        "illustration",
        "style encre",
        "style riso",
        "hand-drawn",
        "dessin animé",
        "anime cette scène",
        "anime la scène",
        "paper ink",
        "look ink",
        "look riso",
    ];
    MARKERS.iter().any(|m| lower.contains(m))
}

/// Short strategy for illustration agents.
pub fn illust_draw_strategy_hint() -> String {
    "PROTOCOLE Illustration : pour une illustration raster, set_brief puis illust.generate_image avec une description de construction adaptée au sujet (pose, proportions, appuis, contacts, composition). L'outil publie les passes en arrière-plan; illust.get donne image_run et pass_previews. Ne pas relancer une génération running. needs_review ne prouve pas la qualité : inspecter visuellement le PNG et signaler les défauts. Ne pas utiliser le score vectoriel pour accepter ce rendu. Si le moteur est absent, rapporter l'erreur sans le remplacer par une recette. Pour une demande explicitement vectorielle, utiliser compose. Un seul auteur, INTERDIT agent.spawn. \
     Format : UNE seule ligne JSON par tour {\"thought\":\"…\",\"action\":\"illust.…\",\"args\":{…}}. \
     Pour la branche vectorielle seulement : set_brief → compose avec une scène complète de contours path (silhouette, \
     masses, appendices, détails distinctifs et accessoires; au moins 8 points par contour) → \
     render_sheet → review structurelle → inspection visuelle → export. \
     Interdit d'empiler plusieurs JSON dans le même tour. \
     Alias acceptés : set_brief/compose/review/export (préfixe illust. ajouté). \
     Règle skill : si ça ne se lit pas sur la planche 240px, redessiner les contours et la composition — ne pas décorer ni demander au puppet de réparer. \
     Après inspection d'un rendu image, corrige ses défauts par illust.refine_image avec une consigne ciblée; compare ensuite image_run.source_png et image_run.candidate_png. Appelle illust.resolve_image avec run_id=image_run.id et keep_candidate=false si la retouche dégrade le rendu, true seulement si elle l'améliore sans perdre de détails demandés. last_png reste l'original jusqu'à ce choix. Ne jamais présenter une description texte comme une image produite."
        .into()
}

/// Pipeline rank for illustration tools (higher = further along).
pub fn illust_pipeline_rank(action: &str) -> u8 {
    match canonicalize_tool_name(action).as_str() {
        "illust.set_brief" | "illust.get" => 1,
        "illust.compose" | "illust.generate_image" | "illust.refine_image" => 2,
        "illust.render_sheet" => 3,
        "illust.review" | "illust.resolve_image" => 4,
        "illust.export" | "illust.animate" => 5,
        "goal.complete" => 6,
        "goal.fail" => 0,
        _ => 0,
    }
}

/// When the model dumps the whole pipeline, keep one action. Prefer advancing
/// past a repeated leading `render_sheet` toward review/export.
pub fn select_illust_turn_action(actions: &[crate::actions::AgentAction]) -> usize {
    if actions.len() <= 1 {
        return 0;
    }
    let first = canonicalize_tool_name(&actions[0].action);
    // Stuck-loop pattern: sheet is listed first, then review/export in the same dump.
    if first == "illust.render_sheet" {
        if let Some(i) = actions.iter().position(|a| {
            matches!(
                canonicalize_tool_name(&a.action).as_str(),
                "illust.review" | "illust.export"
            )
        }) {
            return i;
        }
    }
    // Never finish the goal before the earlier pipeline steps in the same dump.
    if first == "goal.complete" || first == "goal.fail" {
        if let Some(i) = actions.iter().position(|a| {
            let n = canonicalize_tool_name(&a.action);
            n.starts_with("illust.")
        }) {
            return i;
        }
    }
    0
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
    "canvas.text",
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
    "canvas.set_guides",
    "canvas.compose",
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
pub fn merge_canvas_tools(tool_ids: &mut Vec<String>, include: bool, exported: &[String]) {
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

/// Canvas drawing agents must not inherit `user.ask` / spawn tools from `select_tools` "always".
pub fn strip_canvas_blocked_runtime_tools(tools: &mut Vec<ToolDesc>, spec_tool_ids: &[String]) {
    use crate::canvas_scene::agent_has_canvas_tools;
    if !agent_has_canvas_tools(spec_tool_ids) {
        return;
    }
    for blocked in ["user.ask", "agent.spawn", "agent.await"] {
        if spec_tool_ids.iter().any(|t| t == blocked) {
            continue;
        }
        tools.retain(|t| t.name != blocked);
    }
}

/// Drop canvas.* tool ids that the loaded module does not export.
pub fn restrict_canvas_tools(tool_ids: &mut Vec<String>, exported: &[String]) {
    let allowed: std::collections::HashSet<String> =
        filter_canvas_tool_ids(exported).into_iter().collect();
    tool_ids.retain(|t| !t.starts_with("canvas.") || allowed.contains(t));
}

/// Short drawing strategy for canvas agents — only mentions exported tools.
pub fn canvas_draw_strategy_hint(exported: &[String]) -> String {
    if exported.iter().any(|t| t == "canvas.compose") {
        "PROTOCOLE : canvas.get → pour une illustration/objet/personnage, préfère une seule canvas.compose avec profile=illustration et une scène complète (masse principale, partie supérieure, appendices, détails, calques) ; pour graphe/schéma/math, utilise les primitives → relis → canvas.export en dernier.".into()
    } else if exported.iter().any(|t| t == "canvas.path") {
        "PROTOCOLE : canvas.get → lis digest/capture → une seule op (canvas.path pour silhouette, canvas.stroke/rect/ellipse pour détails) → relis → répète → canvas.export en dernier.".into()
    } else {
        "PROTOCOLE : canvas.get → lis digest/capture → une seule op (canvas.stroke/spline/rect/ellipse, fill:true pour remplir) → relis → répète → canvas.export en dernier.".into()
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
        "notes.delete",
        "tasks.create",
        "tasks.list",
        "tasks.update",
        "tasks.complete",
        "fs.read",
        "fs.list",
        "fs.write",
        "web.search",
        "web.browse",
        "system.hardware",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

/// Merge static builtin catalog with discovered module tools.
/// On name collision, discovered manifest tools win over static platform entries.
pub fn merge_catalog_with_discovered(
    static_catalog: &[ToolDesc],
    discovered: &[ToolDesc],
) -> Vec<ToolDesc> {
    let mut out = discovered.to_vec();
    let discovered_names: std::collections::HashSet<&str> =
        discovered.iter().map(|t| t.name.as_str()).collect();
    for t in static_catalog {
        if discovered_names.contains(t.name.as_str()) {
            continue;
        }
        out.push(t.clone());
    }
    out
}

/// Filtre le catalogue selon les ids sélectionnés (+ toujours les runtime de base).
pub fn select_tools(selected: &[String], extra: &[ToolDesc]) -> Vec<ToolDesc> {
    select_tools_mode(selected, extra, false)
}

/// Comme [`select_tools`], avec outils Deep Thinking si `deep`.
pub fn select_tools_mode(selected: &[String], extra: &[ToolDesc], deep: bool) -> Vec<ToolDesc> {
    let catalog = merge_catalog_with_discovered(&builtin_catalog(), extra);
    let module_prefixes = crate::module_discovery::discovered_module_prefixes(extra);
    let mut out: Vec<ToolDesc> = Vec::new();
    let deep_always = [
        "plan.create",
        "plan.update_step",
        "plan.replace_tree",
        "plan.delegate_step",
        "plan.get",
        "plan.append_log",
    ];
    let always_base = [
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
        // Live host snapshot — always on so models stop inventing "no access".
        "system.hardware",
    ];
    let mut always: Vec<&str> = always_base.to_vec();
    if deep {
        always.extend_from_slice(&deep_always);
    } else {
        always.push("plan.update");
    }
    for t in catalog.iter() {
        // Hors mode deep : ne pas exposer les outils plan.* deep
        if !deep && deep_always.contains(&t.name.as_str()) {
            continue;
        }
        if deep && t.name == "plan.update" {
            continue;
        }
        let keep = if selected.is_empty() {
            // Mode permissif : notes + discovered module tools + runtime + fs + extensions
            matches!(t.backend, ToolBackend::Runtime)
                || t.name.starts_with("notes.")
                || module_prefixes
                    .iter()
                    .any(|p| t.name.starts_with(&format!("{p}.")))
                || always.contains(&t.name.as_str())
                || t.name == "fs.read"
                || t.name == "fs.list"
                || t.name == "fs.write"
                || t.name == "mem.context"
                || t.name == "web.search"
                || t.name == "files.generate"
                || t.name == "system.hardware"
                || t.name == "media.image.generate"
                || t.name == "media.audio.generate"
                || t.name.starts_with("device.")
        } else {
            // Mode restreint : always + ids / préfixes explicitement demandés
            let device_selected = selected.iter().any(|s| s.starts_with("device."));
            always.contains(&t.name.as_str())
                || selected
                    .iter()
                    .any(|s| s == &t.name || t.name.starts_with(&format!("{s}.")))
                || (device_selected && t.name.starts_with("device."))
        };
        if keep && !out.iter().any(|x| x.name == t.name) {
            out.push(t.clone());
        }
    }
    if selected
        .iter()
        .any(|s| s == "agent.spawn" || s == "agent.await")
        || selected.is_empty()
        || deep
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

pub const USB_IO_CAP_TOOL_ERROR: &str =
    "device.usb.io est une capacité (grant USB), pas un outil. \
     Liste : device.usb.enumerate. Ouvrir un port COM : device.usb.open {\"device_id\":\"<id de enumerate>\"}. \
     Puis device.usb.read / device.usb.write / device.usb.close. \
     Interdit : shell.run, device.enumerate (caméras).";

/// `device.usb.io` est une capacité (grant), pas un outil — redirige ou rejette clairement.
pub fn resolve_usb_io_cap_tool(
    name: &str,
    args: &serde_json::Value,
) -> Result<(String, serde_json::Value), String> {
    if name != "device.usb.io" {
        return Ok((name.to_string(), args.clone()));
    }
    if let Some(device_id) = args
        .get("device_id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Ok((
            "device.usb.open".into(),
            serde_json::json!({ "device_id": device_id }),
        ));
    }
    if let Some(handle_id) = args
        .get("handle_id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if args.get("data_base64").is_some() {
            return Ok((
                "device.usb.write".into(),
                serde_json::json!({
                    "handle_id": handle_id,
                    "data_base64": args.get("data_base64").cloned().unwrap_or(serde_json::Value::Null)
                }),
            ));
        }
        if args.get("max_bytes").is_some() || args.get("timeout_ms").is_some() {
            let mut out = serde_json::json!({ "handle_id": handle_id });
            if let Some(v) = args.get("max_bytes") {
                out["max_bytes"] = v.clone();
            }
            if let Some(v) = args.get("timeout_ms") {
                out["timeout_ms"] = v.clone();
            }
            return Ok(("device.usb.read".into(), out));
        }
        return Ok((
            "device.usb.close".into(),
            serde_json::json!({ "handle_id": handle_id }),
        ));
    }
    Err(USB_IO_CAP_TOOL_ERROR.into())
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
        "webcam" | "webcam.capture" | "camera.capture" | "camera.snap" | "camera.photo" => {
            "device.camera.capture".into()
        }
        "mic.capture" | "microphone.capture" | "microphone" => "device.mic.capture".into(),
        "usb.list" | "usb.enumerate" | "list.usb" | "usb" => "device.usb.enumerate".into(),
        "usb.open" => "device.usb.open".into(),
        "usb.read" => "device.usb.read".into(),
        "usb.write" => "device.usb.write".into(),
        "usb.close" => "device.usb.close".into(),
        "usb.io" | "device.usb.io" => "device.usb.io".into(),
        // Illustration aliases models invent after reading digests / memory.
        "illust.set" | "illust.brief" | "illust.setbrief" | "illustration.set_brief"
        | "illustration.set" => "illust.set_brief".into(),
        "illust.compose_scene" | "illust.draw" | "illust.run" | "illust.generate"
        | "illustration.compose" | "illustration.run" => "illust.compose".into(),
        "illust.sheet" | "illust.style_sheet" => "illust.render_sheet".into(),
        "illust.check" | "illust.validate" => "illust.review".into(),
        "illust.save" | "illustration.export" => "illust.export".into(),
        other => other.to_string(),
    }
}

/// Bare names models invent once they drop the `illust.` prefix mid-loop.
/// Only apply when the agent actually has illustration tools.
pub fn canonicalize_illust_alias(name: &str) -> String {
    let trimmed = name.trim();
    match trimmed {
        "set_brief" | "setbrief" | "brief" => "illust.set_brief".into(),
        "compose" | "draw" | "run" | "generate" => "illust.compose".into(),
        "render_sheet" | "style_sheet" | "sheet" => "illust.render_sheet".into(),
        "review" | "review_illustration" | "check" | "validate" => "illust.review".into(),
        "export" | "export_still" | "export_png" | "save" => "illust.export".into(),
        "abort" | "cancel" | "give_up" => "goal.fail".into(),
        "get" | "status" => "illust.get".into(),
        other => canonicalize_tool_name(other),
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
            | "device"
            | "usb"
            | "shell"
            | "harness"
            | "illust"
            | "canvas"
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

/// `canvas.*` must always be explicitly granted to an agent.  Unlike ordinary
/// module names, accepting it through the generic module fallback lets a model
/// invoke undocumented operations (notably the legacy `canvas.fill`) that were
/// deliberately removed from the drawing kit.
pub fn canvas_tool_denied_by_allowlist(name: &str, tools: &[ToolDesc]) -> bool {
    name.starts_with("canvas.") && !tools.iter().any(|tool| tool.name == name)
}

/// Alias d'arguments LLM (`prompt` → `text` pour le TTS ; `fill_color` → `color` pour canvas).
pub fn normalize_tool_args(name: &str, args: &serde_json::Value) -> serde_json::Value {
    let mut out = args.clone();
    let Some(obj) = out.as_object_mut() else {
        return out;
    };
    if name.starts_with("canvas.") {
        // Some models nest drawing style even though the canvas module expects
        // top-level fields. Flatten the common form before canonicalizing.
        if let Some(style) = obj.get("style").and_then(|v| v.as_object()).cloned() {
            if !obj.contains_key("color") {
                if let Some(v) = style.get("color").cloned() {
                    obj.insert("color".into(), v);
                }
            }
            if !obj.contains_key("width") {
                if let Some(v) = style.get("width").cloned() {
                    obj.insert("width".into(), v);
                }
            }
        }
        obj.remove("style");
        // `fill_color` is a prompt-facing alias.  The WASM canvas schema
        // accepts the canonical `color` field; keeping both creates a
        // duplicate serde field and makes the module reject the operation.
        if !obj.contains_key("color") {
            if let Some(v) = obj.get("fill_color").cloned() {
                obj.insert("color".into(), v);
            }
        }
        obj.remove("fill_color");
        if name == "canvas.compose" && !obj.contains_key("scene") {
            // Common model wrapper alias. The module contract is `scene`,
            // while local models often call the same payload `scene_spec`.
            if let Some(scene) = obj.remove("scene_spec") {
                obj.insert("scene".into(), scene);
            }
        }
    }
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
        // Here width is the geometric alias for `w`, not the outline width.
        // Keeping it also makes the renderer draw an enormous outline.
        obj.remove("width");
    }
    if name.starts_with("canvas.") && name != "canvas.erase" {
        // A local model often copies a shape's `w` into its stroke `width`.
        // At 0.15 this means a 15%-of-canvas outline, which turns wheels into
        // opaque rings. Keep agent outlines readable; human canvas edits and
        // erasing are not routed through this normalizer.
        const MAX_AGENT_CANVAS_STROKE_WIDTH: f64 = 0.04;
        if let Some(width) = obj.get_mut("width") {
            if width
                .as_f64()
                .is_some_and(|value| value > MAX_AGENT_CANVAS_STROKE_WIDTH)
            {
                *width = serde_json::json!(MAX_AGENT_CANVAS_STROKE_WIDTH);
            }
        }
    }
    if name.starts_with("illust.") {
        normalize_illust_tool_args(name, obj);
    }
    out
}

/// Coerce flat / alias LLM shapes into the Illust* request contracts.
fn normalize_illust_tool_args(name: &str, obj: &mut serde_json::Map<String, serde_json::Value>) {
    if name == "illust.set_brief" {
        let mut brief = match obj.get("brief") {
            Some(serde_json::Value::Object(m)) => m.clone(),
            Some(serde_json::Value::String(s)) => {
                let mut m = serde_json::Map::new();
                m.insert("subject".into(), serde_json::Value::String(s.clone()));
                m
            }
            _ => serde_json::Map::new(),
        };
        for (src, dst) in [
            ("subject", "subject"),
            ("look", "look"),
            ("palette", "palette"),
            ("anchor", "anchor"),
            ("engine", "engine"),
            ("photo", "photo"),
            ("video", "video"),
        ] {
            if !brief.contains_key(dst) {
                if let Some(v) = obj.get(src).cloned() {
                    brief.insert(dst.into(), v);
                }
            }
        }
        // Beats may be strings or {type,desc} objects from the model.
        if !brief.contains_key("beats") {
            if let Some(v) = obj.get("beats").cloned() {
                brief.insert("beats".into(), coerce_illust_beats(v));
            }
        } else if let Some(v) = brief.get("beats").cloned() {
            brief.insert("beats".into(), coerce_illust_beats(v));
        }
        let partial = brief
            .get("subject")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .is_empty();
        // Coerce unknown palette names via look default (serde also falls back).
        let look = brief
            .get("look")
            .and_then(|v| v.as_str())
            .and_then(aos_proto::IllustrationLook::parse)
            .unwrap_or_default();
        if let Some(p) = brief.get("palette").and_then(|v| v.as_str()) {
            let coerced = aos_proto::IllustrationPaletteId::parse_or(p, look);
            brief.insert("palette".into(), serde_json::json!(coerced.as_str()));
        } else if !partial {
            brief.insert(
                "palette".into(),
                serde_json::json!(look.default_palette().as_str()),
            );
        }
        if let Some(l) = brief.get("look").and_then(|v| v.as_str()) {
            let coerced = aos_proto::IllustrationLook::parse(l).unwrap_or_default();
            brief.insert("look".into(), serde_json::json!(coerced.as_str()));
        }
        // Prefer look/palette keywords in subject when the model leaves the UI defaults.
        let subject_l = brief
            .get("subject")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let look_hint = if subject_l.contains("pencil") || subject_l.contains("crayon") {
            Some(aos_proto::IllustrationLook::Pencil)
        } else if subject_l.contains("riso") {
            Some(aos_proto::IllustrationLook::Riso)
        } else if subject_l.contains("screen") || subject_l.contains("sérigraphie") {
            Some(aos_proto::IllustrationLook::Screen)
        } else if subject_l.contains("blueprint") || subject_l.contains("plan ") {
            Some(aos_proto::IllustrationLook::Blueprint)
        } else if subject_l.contains("ink") || subject_l.contains("encre") {
            Some(aos_proto::IllustrationLook::Ink)
        } else {
            None
        };
        if let Some(look) = look_hint {
            brief.insert("look".into(), serde_json::json!(look.as_str()));
            let pal = brief
                .get("palette")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let pal_l = pal.to_ascii_lowercase();
            let pal_mismatched = pal.is_empty()
                || (look == aos_proto::IllustrationLook::Pencil && !pal_l.contains("pencil"))
                || (look == aos_proto::IllustrationLook::Riso && !pal_l.contains("riso"))
                || (look == aos_proto::IllustrationLook::Screen && !pal_l.contains("screen"))
                || (look == aos_proto::IllustrationLook::Blueprint && !pal_l.contains("blueprint"));
            if pal_mismatched {
                brief.insert(
                    "palette".into(),
                    serde_json::json!(look.default_palette().as_str()),
                );
            }
        }
        // Partial updates (beats/anchor only) must not invent empty subject/look defaults.
        if partial {
            brief.remove("subject");
            if !obj.contains_key("look") {
                brief.remove("look");
            }
            if !obj.contains_key("palette") {
                brief.remove("palette");
            }
        }
        obj.insert("brief".into(), serde_json::Value::Object(brief));
        for k in ["subject", "look", "palette", "anchor", "beats"] {
            obj.remove(k);
        }
    }
    // Backward-compatible fallback for callers that omit a spec entirely. New
    // illustration runs should provide authored path parts instead of relying
    // on the procedural puppet recipes.
    if name == "illust.compose" {
        if !obj.contains_key("spec") && !obj.contains_key("parts") {
            obj.insert(
                "spec".into(),
                serde_json::json!({ "parts": [] }),
            );
        }
    }
    if name == "illust.compose" {
        // Flat parts at top-level → wrap as spec.
        if !obj.contains_key("spec") {
            if obj.contains_key("parts") {
                let mut spec = serde_json::Map::new();
                for k in [
                    "parts",
                    "key_drawings",
                    "pose",
                    "camera",
                    "mode",
                    "brief",
                    "seed",
                    "show_construction",
                ] {
                    if let Some(v) = obj.remove(k) {
                        spec.insert(k.into(), v);
                    }
                }
                obj.insert("spec".into(), serde_json::Value::Object(spec));
            }
        }
        if let Some(serde_json::Value::Object(spec)) = obj.get_mut("spec") {
            // Flat subject/look/palette on spec → nested brief.
            let mut brief = match spec.get("brief") {
                Some(serde_json::Value::Object(m)) => m.clone(),
                Some(serde_json::Value::String(s)) => {
                    let mut m = serde_json::Map::new();
                    m.insert("subject".into(), serde_json::Value::String(s.clone()));
                    m
                }
                _ => serde_json::Map::new(),
            };
            for key in ["subject", "look", "palette", "anchor", "beats"] {
                if !brief.contains_key(key) {
                    if let Some(v) = spec.remove(key) {
                        brief.insert(key.into(), v);
                    }
                } else {
                    spec.remove(key);
                }
            }
            let look = brief
                .get("look")
                .and_then(|v| v.as_str())
                .and_then(aos_proto::IllustrationLook::parse)
                .unwrap_or_default();
            if let Some(p) = brief.get("palette").and_then(|v| v.as_str()) {
                let coerced = aos_proto::IllustrationPaletteId::parse_or(p, look);
                brief.insert("palette".into(), serde_json::json!(coerced.as_str()));
            }
            if let Some(l) = brief.get("look").and_then(|v| v.as_str()) {
                let coerced = aos_proto::IllustrationLook::parse(l).unwrap_or_default();
                brief.insert("look".into(), serde_json::json!(coerced.as_str()));
            }
            if !brief.is_empty() {
                spec.insert("brief".into(), serde_json::Value::Object(brief));
            }

            let bound_ids: std::collections::HashSet<String> = spec.get("joint_bindings")
                .and_then(|v| v.as_object())
                .map(|bindings| bindings.keys().cloned().collect())
                .unwrap_or_default();
            if let Some(serde_json::Value::Array(parts)) = spec.get_mut("parts") {
                let n = parts.len().max(1);
                for (i, part) in parts.iter_mut().enumerate() {
                    let Some(p) = part.as_object_mut() else {
                        continue;
                    };
                    let role = p
                        .get("role")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    if p.get("id").and_then(|v| v.as_str()).unwrap_or("").is_empty() {
                        let id = if role.is_empty() {
                            format!("part{i}")
                        } else {
                            format!("{role}_{i}")
                        };
                        p.insert("id".into(), serde_json::Value::String(id));
                    }
                    let bound = p.get("id").and_then(|v| v.as_str())
                        .map(|id| bound_ids.contains(id)).unwrap_or(false);
                    if let Some(geom) = p.get_mut("geometry").and_then(|g| g.as_object_mut()) {
                        if !geom.contains_key("kind") {
                            if let Some(t) = geom.remove("type") {
                                geom.insert("kind".into(), t);
                            }
                        }
                        // Local joint coordinates may legitimately exceed 1 or
                        // be negative. They are not pixel/percentage coordinates.
                        if !bound {
                            normalize_illust_part_geometry(geom, &role, i, n);
                        }
                    } else {
                        // Models often send role+description only — invent a readable layout.
                        let (x, y, w, h) = layout_for_illust_role(&role, i, n);
                        p.insert(
                            "geometry".into(),
                            serde_json::json!({
                                "kind": "ellipse",
                                "x": x,
                                "y": y,
                                "w": w,
                                "h": h,
                                "rotation": 0.0
                            }),
                        );
                    }
                    if !p.contains_key("fill") {
                        p.insert("fill".into(), serde_json::json!(true));
                    }
                    if !p.contains_key("outline") {
                        p.insert("outline".into(), serde_json::json!(true));
                    }
                    if !p.contains_key("fill_index") {
                        let id = p
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        p.insert(
                            "fill_index".into(),
                            serde_json::json!(default_illust_fill_index(&role, &id, i)),
                        );
                    }
                    if !p.contains_key("seed") {
                        p.insert("seed".into(), serde_json::json!(i as u32 + 1));
                    }
                    // Drop free-text fields the schema rejects.
                    p.remove("description");
                    p.remove("label");
                    p.remove("name");
                }
            }
        }
    }
}

fn coerce_illust_beats(v: serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::Array(items) => {
            let out: Vec<serde_json::Value> = items
                .into_iter()
                .map(|item| match item {
                    serde_json::Value::String(s) => serde_json::Value::String(s),
                    serde_json::Value::Object(m) => {
                        let ty = m
                            .get("type")
                            .or_else(|| m.get("role"))
                            .or_else(|| m.get("id"))
                            .and_then(|x| x.as_str())
                            .unwrap_or("beat");
                        let desc = m
                            .get("desc")
                            .or_else(|| m.get("description"))
                            .or_else(|| m.get("text"))
                            .and_then(|x| x.as_str())
                            .unwrap_or("");
                        if desc.is_empty() {
                            serde_json::Value::String(ty.to_string())
                        } else {
                            serde_json::Value::String(format!("{ty}: {desc}"))
                        }
                    }
                    other => serde_json::Value::String(other.to_string()),
                })
                .collect();
            serde_json::Value::Array(out)
        }
        serde_json::Value::String(s) => {
            serde_json::Value::Array(vec![serde_json::Value::String(s)])
        }
        other => serde_json::Value::Array(vec![serde_json::Value::String(other.to_string())]),
    }
}

/// Fix LLM geometry shapes before CBOR encode (path without points, missing bbox, …).
fn normalize_illust_part_geometry(
    geom: &mut serde_json::Map<String, serde_json::Value>,
    role: &str,
    index: usize,
    total: usize,
) {
    let kind = geom
        .get("kind")
        .and_then(|v| v.as_str())
        .unwrap_or("ellipse")
        .to_string();
    let points_ok = geom
        .get("points")
        .and_then(|v| v.as_array())
        .map(|a| a.len() >= 2)
        .unwrap_or(false);

    // Models often emit `path` with only a bbox (x/y/w/h) and no `points`.
    if kind == "path" && !points_ok {
        let (lx, ly, lw, lh) = layout_for_illust_role(role, index, total);
        let x = num_f32(geom.get("x")).unwrap_or(lx);
        let y = num_f32(geom.get("y")).unwrap_or(ly);
        let w = num_f32(geom.get("w")).unwrap_or(lw);
        let h = num_f32(geom.get("h")).unwrap_or(lh);
        let rot = num_f32(geom.get("rotation")).unwrap_or(0.0);
        geom.clear();
        geom.insert("kind".into(), serde_json::json!("ellipse"));
        geom.insert("x".into(), serde_json::json!(x));
        geom.insert("y".into(), serde_json::json!(y));
        geom.insert("w".into(), serde_json::json!(w));
        geom.insert("h".into(), serde_json::json!(h));
        geom.insert("rotation".into(), serde_json::json!(rot));
        rescale_illust_bbox_fields(geom);
        return;
    }

    if kind == "path" && points_ok {
        rescale_illust_path_points(geom);
        return;
    }

    if matches!(kind.as_str(), "ellipse" | "rect") {
        let (lx, ly, lw, lh) = layout_for_illust_role(role, index, total);
        if num_f32(geom.get("x")).is_none() {
            geom.insert("x".into(), serde_json::json!(lx));
        }
        if num_f32(geom.get("y")).is_none() {
            geom.insert("y".into(), serde_json::json!(ly));
        }
        if num_f32(geom.get("w")).is_none() {
            geom.insert("w".into(), serde_json::json!(lw));
        }
        if num_f32(geom.get("h")).is_none() {
            geom.insert("h".into(), serde_json::json!(lh));
        }
        if !geom.contains_key("rotation") {
            geom.insert("rotation".into(), serde_json::json!(0.0));
        }
        rescale_illust_bbox_fields(geom);
    }
}

/// LLMs often send 0..100 (%) or ~1024px instead of normalized 0..1.
fn illust_coord_divisor(samples: &[f32]) -> f32 {
    let max_abs = samples
        .iter()
        .copied()
        .map(f32::abs)
        .fold(0.0f32, f32::max);
    if !max_abs.is_finite() || max_abs <= 1.5 {
        1.0
    } else if max_abs <= 100.5 {
        100.0
    } else if max_abs <= 2048.0 {
        1024.0
    } else {
        max_abs
    }
}

fn rescale_illust_bbox_fields(geom: &mut serde_json::Map<String, serde_json::Value>) {
    let x = num_f32(geom.get("x")).unwrap_or(0.0);
    let y = num_f32(geom.get("y")).unwrap_or(0.0);
    let w = num_f32(geom.get("w")).unwrap_or(0.1);
    let h = num_f32(geom.get("h")).unwrap_or(0.1);
    let div = illust_coord_divisor(&[x, y, w, h, x + w, y + h]);
    if (div - 1.0).abs() < f32::EPSILON {
        return;
    }
    geom.insert("x".into(), serde_json::json!(x / div));
    geom.insert("y".into(), serde_json::json!(y / div));
    geom.insert("w".into(), serde_json::json!((w / div).max(0.01)));
    geom.insert("h".into(), serde_json::json!((h / div).max(0.01)));
}

fn rescale_illust_path_points(geom: &mut serde_json::Map<String, serde_json::Value>) {
    let Some(serde_json::Value::Array(pts)) = geom.get("points").cloned() else {
        return;
    };
    let mut samples = Vec::new();
    for p in &pts {
        if let Some(o) = p.as_object() {
            if let Some(x) = num_f32(o.get("x")) {
                samples.push(x);
            }
            if let Some(y) = num_f32(o.get("y")) {
                samples.push(y);
            }
        }
    }
    let div = illust_coord_divisor(&samples);
    if (div - 1.0).abs() < f32::EPSILON {
        return;
    }
    let scaled: Vec<serde_json::Value> = pts
        .into_iter()
        .map(|p| {
            let Some(mut o) = p.as_object().cloned() else {
                return p;
            };
            if let Some(x) = num_f32(o.get("x")) {
                o.insert("x".into(), serde_json::json!(x / div));
            }
            if let Some(y) = num_f32(o.get("y")) {
                o.insert("y".into(), serde_json::json!(y / div));
            }
            serde_json::Value::Object(o)
        })
        .collect();
    geom.insert("points".into(), serde_json::Value::Array(scaled));
}

fn default_illust_fill_index(role: &str, id: &str, index: usize) -> u64 {
    let s = format!("{role} {id}").to_ascii_lowercase();
    if s.contains("sand") || s.contains("sable") || s.contains("beach") || s.contains("plage") {
        3
    } else if s.contains("sea") || s.contains("mer") || s.contains("ocean") || s.contains("eau") {
        1
    } else if s.contains("sky") || s.contains("ciel") {
        0
    } else if s.contains("sun") || s.contains("soleil") || s.contains("helmet") || s.contains("casque")
    {
        5
    } else if s.contains("bike") || s.contains("velo") || s.contains("vélo") || s.contains("wheel")
    {
        7
    } else if matches!(
        role.to_ascii_lowercase().as_str(),
        "background" | "bg"
    ) {
        0
    } else {
        (index as u64 % 4).max(1)
    }
}

fn num_f32(v: Option<&serde_json::Value>) -> Option<f32> {
    match v? {
        serde_json::Value::Number(n) => n.as_f64().map(|f| f as f32),
        serde_json::Value::String(s) => s.parse::<f32>().ok(),
        _ => None,
    }
}

fn layout_for_illust_role(role: &str, index: usize, total: usize) -> (f32, f32, f32, f32) {
    let r = role.to_ascii_lowercase();
    match r.as_str() {
        "background" | "bg" | "sky" | "plage" | "beach" | "ocean" | "mer" => {
            (0.05, 0.55, 0.9, 0.4)
        }
        "ground" | "sable" | "sand" => (0.05, 0.7, 0.9, 0.25),
        "sun" | "soleil" | "moon" => (0.72, 0.08, 0.16, 0.16),
        "body" | "main" | "subject" | "masse" => (0.32, 0.28, 0.36, 0.42),
        "bike" | "velo" | "vélo" | "cycle" => (0.22, 0.48, 0.56, 0.28),
        "wheel" | "roue" => {
            if index % 2 == 0 {
                (0.24, 0.58, 0.16, 0.16)
            } else {
                (0.58, 0.58, 0.16, 0.16)
            }
        }
        _ => {
            let col = (index % 3) as f32;
            let row = (index / 3) as f32;
            let cols = 3.0_f32.min(total as f32).max(1.0);
            let x = 0.12 + col * (0.7 / cols);
            let y = 0.18 + row * 0.28;
            (x, y, 0.22, 0.22)
        }
    }
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
    fn chat_template_tool_definitions_use_openai_function_shape() {
        let input = ToolDesc {
            name: "canvas.get".into(),
            description: "Lire la scène".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {"after_seq": {"type": "integer"}}
            }),
            backend: ToolBackend::Runtime,
            required_caps: vec![],
        };
        let defs = chat_template_tool_definitions(&[input]);
        assert_eq!(defs[0]["type"], "function");
        assert_eq!(defs[0]["function"]["name"], "canvas.get");
        assert_eq!(
            defs[0]["function"]["parameters"]["properties"]["after_seq"]["type"],
            "integer"
        );
    }

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
        assert!(t.iter().any(|x| x.name == "system.hardware"));
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
    fn strip_canvas_blocked_runtime_tools_removes_user_ask() {
        let spec = vec!["canvas.get".into(), "canvas.set_style".into()];
        let mut tools = select_tools(&spec, &[]);
        assert!(tools.iter().any(|t| t.name == "user.ask"));
        strip_canvas_blocked_runtime_tools(&mut tools, &spec);
        assert!(!tools.iter().any(|t| t.name == "user.ask"));
        assert!(!tools.iter().any(|t| t.name == "agent.spawn"));
        assert!(tools.iter().any(|t| t.name == "canvas.set_style"));
    }

    #[test]
    fn strip_illust_blocked_runtime_tools_removes_spawn() {
        let spec = vec!["illust.get".into(), "illust.set_brief".into()];
        let mut tools = select_tools_mode(&spec, &[], true);
        assert!(tools.iter().any(|t| t.name == "agent.spawn"));
        strip_illust_blocked_runtime_tools(&mut tools, &spec);
        assert!(!tools.iter().any(|t| t.name == "agent.spawn"));
        assert!(!tools.iter().any(|t| t.name == "user.ask"));
        assert!(tools.iter().any(|t| t.name == "illust.set_brief"));
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
    fn normalize_canvas_path_fill_color_alias_to_color() {
        let args = normalize_tool_args(
            "canvas.path",
            &serde_json::json!({
                "session_id": "s1",
                "points": [{"x": 0.1, "y": 0.2}, {"x": 0.5, "y": 0.5}, {"x": 0.9, "y": 0.2}],
                "fill_color": "#8B7355"
            }),
        );
        assert_eq!(args["color"], "#8B7355");
        assert!(args.get("fill_color").is_none());
    }

    #[test]
    fn normalize_canvas_style_object_to_top_level_fields() {
        let args = normalize_tool_args(
            "canvas.path",
            &serde_json::json!({
                "points": [{"x": 0.1, "y": 0.2}],
                "style": {"color": "#333333", "width": 0.02}
            }),
        );
        assert_eq!(args["color"], "#333333");
        assert_eq!(args["width"], 0.02);
        assert!(args.get("style").is_none());
    }

    #[test]
    fn normalize_canvas_compose_scene_spec_alias() {
        let args = normalize_tool_args(
            "canvas.compose",
            &serde_json::json!({
                "scene_spec": {
                    "version": 1,
                    "profile": "illustration",
                    "subject": "chat",
                    "elements": []
                }
            }),
        );
        assert_eq!(args["scene"]["subject"], "chat");
        assert!(args.get("scene_spec").is_none());
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
    fn normalize_canvas_outline_width_prevents_opaque_wheels() {
        let args = normalize_tool_args(
            "canvas.ellipse",
            &serde_json::json!({
                "x": 0.25, "y": 0.65, "w": 0.15, "h": 0.08,
                "width": 0.15, "fill": false
            }),
        );
        assert_eq!(args["width"], 0.04);
        let aliases = normalize_tool_args(
            "canvas.ellipse",
            &serde_json::json!({"x": 0.25, "y": 0.65, "width": 0.15, "height": 0.08}),
        );
        assert_eq!(aliases["w"], 0.15);
        assert!(aliases.get("width").is_none());
    }

    #[test]
    fn discovered_tasks_catalog_matches_frozen_contract() {
        use crate::module_discovery::discover_module_tools_from_list;
        use aos_proto::tasks_contract::{INVOKE_CAP, TOOL_IDS};
        use aos_proto::ModuleInfo;

        let module = ModuleInfo {
            name: "tasks".into(),
            version: "1.0.0".into(),
            granted_caps: vec![INVOKE_CAP.into()],
            tools: TOOL_IDS.iter().map(|s| s.to_string()).collect(),
            quarantined: false,
            ui_mode: None,
            ui_title: None,
        };
        let discovered = discover_module_tools_from_list(&[module], |_| None);
        let selected: Vec<String> = TOOL_IDS.iter().map(|s| s.to_string()).collect();
        let tools = select_tools(&selected, &discovered);
        for id in TOOL_IDS {
            let tool = tools
                .iter()
                .find(|t| t.name == *id)
                .unwrap_or_else(|| panic!("discovered catalog missing {id}"));
            assert_eq!(tool.required_caps, vec![INVOKE_CAP.to_string()]);
        }
    }

    #[test]
    fn merge_catalog_prefers_discovered_over_static_collision() {
        let static_tool = ToolDesc {
            name: "tasks.list".into(),
            description: "static".into(),
            input_schema: serde_json::json!({}),
            backend: ToolBackend::Module,
            required_caps: vec!["tool.invoke:tasks".into()],
        };
        let discovered = ToolDesc {
            name: "tasks.list".into(),
            description: "from manifest".into(),
            input_schema: serde_json::json!({"type":"object"}),
            backend: ToolBackend::Module,
            required_caps: vec!["tool.invoke:tasks".into()],
        };
        let merged = merge_catalog_with_discovered(&[static_tool], &[discovered]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].description, "from manifest");
    }

    #[test]
    fn static_builtin_catalog_has_no_tasks_tools() {
        let catalog = builtin_catalog();
        assert!(!catalog.iter().any(|t| t.name.starts_with("tasks.")));
    }

    #[test]
    fn static_builtin_catalog_has_no_create_tools() {
        let catalog = builtin_catalog();
        assert!(!catalog.iter().any(|t| t.name.starts_with("create.")));
    }

    #[test]
    fn discovered_create_catalog_matches_frozen_contract() {
        use crate::module_discovery::discover_module_tools_from_list;
        use aos_proto::create_contract::{INVOKE_CAP, TOOL_IDS};
        use aos_proto::ModuleInfo;

        let module = ModuleInfo {
            name: "create".into(),
            version: "1.0.0".into(),
            granted_caps: vec![INVOKE_CAP.into()],
            tools: TOOL_IDS.iter().map(|s| s.to_string()).collect(),
            quarantined: false,
            ui_mode: None,
            ui_title: None,
        };
        let discovered = discover_module_tools_from_list(&[module], |_| None);
        let selected: Vec<String> = TOOL_IDS.iter().map(|s| s.to_string()).collect();
        let tools = select_tools(&selected, &discovered);
        for id in TOOL_IDS {
            let tool = tools
                .iter()
                .find(|t| t.name == *id)
                .unwrap_or_else(|| panic!("discovered catalog missing {id}"));
            assert_eq!(tool.required_caps, vec![INVOKE_CAP.to_string()]);
        }
    }

    #[test]
    fn media_image_generate_available_without_create_module() {
        let catalog = builtin_catalog();
        assert!(
            catalog.iter().any(|t| t.name == "media.image.generate"),
            "platform image generation must stay in builtin catalog without Create installed"
        );
        assert!(!catalog.iter().any(|t| t.name.starts_with("create.")));
        let tools = select_tools(&["media.image.generate".into()], &[]);
        assert!(tools.iter().any(|t| t.name == "media.image.generate"));
        let caps = caps_for_tools(&tools, &[]);
        assert!(caps.iter().any(|c| c == "media.generate"));
    }

    #[test]
    fn default_agent_tools_include_system_hardware() {
        let ids = default_agent_tools();
        assert!(ids.iter().any(|t| t == "system.hardware"));
        let tools = select_tools(&ids, &[]);
        assert!(tools.iter().any(|t| t.name == "system.hardware"));
    }

    #[test]
    fn default_agent_tools_include_tasks_when_module_discovered() {
        use crate::module_discovery::discover_module_tools_from_list;
        use aos_proto::tasks_contract::{INVOKE_CAP, TOOL_IDS};
        use aos_proto::ModuleInfo;

        let module = ModuleInfo {
            name: "tasks".into(),
            version: "1.0.0".into(),
            granted_caps: vec![INVOKE_CAP.into()],
            tools: TOOL_IDS.iter().map(|s| s.to_string()).collect(),
            quarantined: false,
            ui_mode: None,
            ui_title: None,
        };
        let discovered = discover_module_tools_from_list(&[module], |_| None);
        let ids = default_agent_tools();
        let tools = select_tools(&ids, &discovered);
        let caps = caps_for_tools(&tools, &[]);
        assert!(caps.iter().any(|c| c == "tool.invoke:tasks"));
        assert!(tools.iter().any(|t| t.name == "tasks.create"));
    }

    #[test]
    fn default_agent_tools_grant_notes_tasks_fs_web() {
        use crate::module_discovery::discover_module_tools_from_list;
        use aos_proto::tasks_contract::{INVOKE_CAP, TOOL_IDS};
        use aos_proto::ModuleInfo;

        let module = ModuleInfo {
            name: "tasks".into(),
            version: "1.0.0".into(),
            granted_caps: vec![INVOKE_CAP.into()],
            tools: TOOL_IDS.iter().map(|s| s.to_string()).collect(),
            quarantined: false,
            ui_mode: None,
            ui_title: None,
        };
        let discovered = discover_module_tools_from_list(&[module], |_| None);
        let ids = default_agent_tools();
        let tools = select_tools(&ids, &discovered);
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
        let tools = select_tools(&["canvas.rect".into(), "canvas.ellipse".into()], &[]);
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
        assert_eq!(
            canonicalize_tool_name("audio.generate"),
            "media.audio.generate"
        );
        assert_eq!(
            canonicalize_tool_name("tts.generate"),
            "media.audio.generate"
        );
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

    #[test]
    fn device_capture_tools_are_native_and_catalogued() {
        assert_eq!(
            canonicalize_tool_name("webcam.capture"),
            "device.camera.capture"
        );
        assert_eq!(
            canonicalize_tool_name("camera.snap"),
            "device.camera.capture"
        );
        assert_eq!(canonicalize_tool_name("mic.capture"), "device.mic.capture");
        assert!(!is_module_fallback_candidate("device.camera.capture"));
        let tools = select_tools(&["device.camera.capture".into()], &[]);
        assert!(tools.iter().any(|t| t.name == "device.camera.capture"));
        assert!(tools.iter().any(|t| t.name == "device.enumerate"));
        assert!(tools.iter().any(|t| t.name == "device.mic.capture"));
        let permissive = select_tools(&[], &[]);
        assert!(permissive.iter().any(|t| t.name == "device.camera.capture"));
        let (kind, _, _) = classify_action("device.camera.capture", &[], &[]);
        assert_eq!(kind, "native");
    }

    #[test]
    fn harness_run_is_opt_in_native() {
        assert!(!is_module_fallback_candidate("harness.run"));
        let permissive = select_tools(&[], &[]);
        assert!(!permissive.iter().any(|t| t.name == "harness.run"));
        let tools = select_tools(&["harness.run".into()], &[]);
        assert!(tools.iter().any(|t| t.name == "harness.run"));
        let (kind, _, _) = classify_action("harness.run", &tools, &[]);
        assert_eq!(kind, "native");
        assert!(caps_for_tools(&tools, &[]).contains(&"harness.run".to_string()));
    }

    #[test]
    fn resolve_usb_io_cap_tool_rejects_bare_cap_name() {
        let err =
            resolve_usb_io_cap_tool("device.usb.io", &serde_json::json!({})).expect_err("bare cap");
        assert!(err.contains("device.usb.enumerate"));
        assert!(err.contains("device.usb.open"));
    }

    #[test]
    fn resolve_usb_io_cap_tool_canonicalizes_open() {
        let (name, args) = resolve_usb_io_cap_tool(
            "device.usb.io",
            &serde_json::json!({"device_id": "win:Serial:COM3"}),
        )
        .expect("open redirect");
        assert_eq!(name, "device.usb.open");
        assert_eq!(args["device_id"], "win:Serial:COM3");
    }

    #[test]
    fn canonicalize_usb_aliases() {
        assert_eq!(
            canonicalize_tool_name("tool.invoke:usb.list"),
            "device.usb.enumerate"
        );
        assert_eq!(
            canonicalize_tool_name("usb.enumerate"),
            "device.usb.enumerate"
        );
        assert_eq!(canonicalize_tool_name("list.usb"), "device.usb.enumerate");
        assert_eq!(canonicalize_tool_name("usb"), "device.usb.enumerate");
        assert_eq!(canonicalize_tool_name("usb.open"), "device.usb.open");
        assert_eq!(canonicalize_tool_name("usb.read"), "device.usb.read");
        assert_eq!(canonicalize_tool_name("usb.write"), "device.usb.write");
        assert_eq!(canonicalize_tool_name("usb.close"), "device.usb.close");
        assert_eq!(canonicalize_tool_name("device.usb.io"), "device.usb.io");
        assert!(!is_module_fallback_candidate("usb.list"));
        assert!(!is_module_fallback_candidate("shell.run"));
        assert!(reserved_tool_prefix("usb"));
        assert!(reserved_tool_prefix("shell"));
        assert!(reserved_tool_prefix("harness"));
        assert!(!is_module_fallback_candidate("harness.run"));
    }

    #[test]
    fn canvas_tools_cannot_bypass_the_selected_toolkit() {
        let tools = select_tools(&["canvas.path".into()], &[]);
        assert!(!canvas_tool_denied_by_allowlist("canvas.path", &tools));
        assert!(canvas_tool_denied_by_allowlist("canvas.fill", &tools));
        assert!(!canvas_tool_denied_by_allowlist("notes.create", &tools));
    }

    #[test]
    fn illust_set_brief_normalizes_flat_string_brief_and_unknown_palette() {
        let args = normalize_tool_args(
            "illust.set_brief",
            &serde_json::json!({
                "brief": "Un pingouin sur un vélo",
                "look": "ink",
                "palette": "warm_sunny_beach"
            }),
        );
        assert_eq!(args["brief"]["subject"], "Un pingouin sur un vélo");
        assert_eq!(args["brief"]["look"], "ink");
        assert_eq!(args["brief"]["palette"], "paperInk");
        assert!(args.get("look").is_none());
        assert!(args.get("palette").is_none());
    }

    #[test]
    fn illust_compose_wraps_flat_parts_and_geometry_type() {
        let args = normalize_tool_args(
            "illust.compose",
            &serde_json::json!({
                "parts": [{
                    "id": "body",
                    "role": "body",
                    "geometry": {"type": "ellipse", "x": 0.2, "y": 0.2, "w": 0.4, "h": 0.5}
                }]
            }),
        );
        assert!(args.get("spec").is_some());
        assert_eq!(args["spec"]["parts"][0]["geometry"]["kind"], "ellipse");
    }

    #[test]
    fn illust_compose_synthesizes_id_and_geometry_from_description_parts() {
        let args = normalize_tool_args(
            "illust.compose",
            &serde_json::json!({
                "spec": {
                    "subject": "pingouin sur un vélo",
                    "look": "pencil",
                    "palette": "pencilMinimal",
                    "parts": [
                        {"role":"main","description":"pingouin"},
                        {"role":"body","description":"vélo"},
                        {"role":"background","description":"plage"}
                    ]
                }
            }),
        );
        let parts = args["spec"]["parts"].as_array().unwrap();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0]["id"], "main_0");
        assert_eq!(parts[0]["geometry"]["kind"], "ellipse");
        assert!(parts[0].get("description").is_none());
        assert_eq!(args["spec"]["brief"]["subject"], "pingouin sur un vélo");
        assert_eq!(args["spec"]["brief"]["palette"], "pencilMinimal");
        assert!(args["spec"].get("subject").is_none());
    }

    #[test]
    fn illust_compose_coerces_path_bbox_without_points_to_ellipse() {
        let args = normalize_tool_args(
            "illust.compose",
            &serde_json::json!({
                "spec": {
                    "parts": [{
                        "id": "1",
                        "role": "body",
                        "geometry": {"kind":"path","x":0.3,"y":0.4,"w":0.25,"h":0.25}
                    }]
                }
            }),
        );
        let g = &args["spec"]["parts"][0]["geometry"];
        assert_eq!(g["kind"], "ellipse");
        assert!((g["x"].as_f64().unwrap() - 0.3).abs() < 1e-6);
        assert!((g["w"].as_f64().unwrap() - 0.25).abs() < 1e-6);
        assert!(g.get("points").is_none());
    }

    #[test]
    fn illust_compose_preserves_joint_local_coordinates() {
        let geometry = serde_json::json!({"kind":"path", "closed":true,
            "points":[{"x":0.0,"y":-0.1},{"x":12.0,"y":0.1},{"x":1.0,"y":0.2}]});
        let normalized = normalize_tool_args("illust.compose", &serde_json::json!({
            "spec": {"joint_bindings":{"sleeve":["elbow","wrist"]},
            "parts":[{"id":"sleeve","role":"clothing","geometry":geometry.clone()}]}
        }));
        assert_eq!(normalized["spec"]["parts"][0]["geometry"], geometry);
    }

    #[test]
    fn illust_compose_rescales_percent_coords_to_unit() {
        let args = normalize_tool_args(
            "illust.compose",
            &serde_json::json!({
                "spec": {
                    "parts": [
                        {"id":"bg_sky","role":"background","geometry":{"kind":"rect","x":0,"y":0,"w":100,"h":60}},
                        {"id":"penguin_body","role":"main","geometry":{"kind":"ellipse","x":45,"y":40,"w":10,"h":20}}
                    ]
                }
            }),
        );
        let sky = &args["spec"]["parts"][0]["geometry"];
        let penguin = &args["spec"]["parts"][1]["geometry"];
        assert!((sky["w"].as_f64().unwrap() - 1.0).abs() < 1e-6);
        assert!((sky["h"].as_f64().unwrap() - 0.6).abs() < 1e-6);
        assert!((penguin["x"].as_f64().unwrap() - 0.45).abs() < 1e-6);
        assert!((penguin["w"].as_f64().unwrap() - 0.1).abs() < 1e-6);
        assert_eq!(args["spec"]["parts"][0]["fill_index"], 0);
        assert_eq!(args["spec"]["parts"][1]["fill_index"], 1);
    }

    #[test]
    fn illust_compose_rescales_pixel_coords_to_unit() {
        let args = normalize_tool_args(
            "illust.compose",
            &serde_json::json!({
                "spec": {
                    "parts": [{
                        "id":"penguin","role":"main",
                        "geometry":{"kind":"rect","x":350,"y":350,"w":320,"h":280}
                    }]
                }
            }),
        );
        let g = &args["spec"]["parts"][0]["geometry"];
        assert!((g["x"].as_f64().unwrap() - 350.0 / 1024.0).abs() < 1e-5);
        assert!((g["w"].as_f64().unwrap() - 320.0 / 1024.0).abs() < 1e-5);
    }

    #[test]
    fn illust_image_tool_is_available_without_becoming_vector_compose() {
        let mut ids = Vec::new();
        merge_illust_tools(&mut ids, true);
        assert!(ids.iter().any(|id| id == "illust.generate_image"));
        assert_eq!(canonicalize_tool_name("illust.generate_image"), "illust.generate_image");
        let args = serde_json::json!({"construction":"quadruped curled on a cushion"});
        assert_eq!(normalize_tool_args("illust.generate_image", &args), args);
        assert!(ids.iter().any(|id| id == "illust.refine_image"));
        assert!(ids.iter().any(|id| id == "illust.resolve_image"));
        let args = serde_json::json!({"correction":"remove construction circles"});
        assert_eq!(normalize_tool_args("illust.refine_image", &args), args);
    }

    #[test]
    fn illust_set_aliases_canonicalize() {
        assert_eq!(canonicalize_tool_name("illust.set"), "illust.set_brief");
        assert_eq!(canonicalize_tool_name("illust.brief"), "illust.set_brief");
        assert_eq!(canonicalize_tool_name("illustration.compose"), "illust.compose");
        assert_eq!(canonicalize_tool_name("illust.run"), "illust.compose");
        assert_eq!(canonicalize_tool_name("illust.generate"), "illust.compose");
    }

    #[test]
    fn illust_bare_aliases_only_via_illust_helper() {
        assert_eq!(canonicalize_illust_alias("set_brief"), "illust.set_brief");
        assert_eq!(canonicalize_illust_alias("compose"), "illust.compose");
        assert_eq!(canonicalize_illust_alias("review"), "illust.review");
        assert_eq!(canonicalize_illust_alias("export"), "illust.export");
        assert_eq!(canonicalize_illust_alias("abort"), "goal.fail");
        // Global canonicalize must not steal bare names from other agents.
        assert_eq!(canonicalize_tool_name("compose"), "compose");
        assert_eq!(canonicalize_tool_name("export"), "export");
    }

    #[test]
    fn select_illust_turn_skips_leading_sheet_when_review_present() {
        let actions = vec![
            crate::actions::AgentAction {
                thought: String::new(),
                action: "illust.render_sheet".into(),
                args: serde_json::json!({}),
            },
            crate::actions::AgentAction {
                thought: String::new(),
                action: "illust.review".into(),
                args: serde_json::json!({}),
            },
            crate::actions::AgentAction {
                thought: String::new(),
                action: "illust.export".into(),
                args: serde_json::json!({}),
            },
        ];
        assert_eq!(select_illust_turn_action(&actions), 1);
    }

    #[test]
    fn select_illust_turn_keeps_leading_set_brief() {
        let actions = vec![
            crate::actions::AgentAction {
                thought: String::new(),
                action: "illust.set_brief".into(),
                args: serde_json::json!({}),
            },
            crate::actions::AgentAction {
                thought: String::new(),
                action: "illust.compose".into(),
                args: serde_json::json!({}),
            },
            crate::actions::AgentAction {
                thought: String::new(),
                action: "goal.complete".into(),
                args: serde_json::json!({}),
            },
        ];
        assert_eq!(select_illust_turn_action(&actions), 0);
    }

    #[test]
    fn illust_run_empty_args_become_empty_compose_spec() {
        let args = normalize_tool_args("illust.compose", &serde_json::json!({}));
        assert!(args["spec"]["parts"].as_array().unwrap().is_empty());
    }

    #[test]
    fn illust_set_brief_partial_beats_omit_empty_subject() {
        let args = normalize_tool_args(
            "illust.set_brief",
            &serde_json::json!({
                "beats": [
                    {"type":"body","desc":"Corps allongé","pos":"center","weight":0.9},
                    {"type":"head","desc":"Tête posée","pos":"lower_left"}
                ]
            }),
        );
        assert!(args["brief"].get("subject").is_none());
        let beats = args["brief"]["beats"].as_array().unwrap();
        assert_eq!(beats.len(), 2);
        assert!(beats[0].as_str().unwrap().contains("body"));
        assert!(beats[1].as_str().unwrap().contains("head"));
    }

    #[test]
    fn illust_set_brief_infers_pencil_from_subject() {
        let args = normalize_tool_args(
            "illust.set_brief",
            &serde_json::json!({
                "brief": {
                    "subject": "chat qui s'étire sur un coussin style pencil",
                    "look": "ink",
                    "palette": "paperInk"
                }
            }),
        );
        assert_eq!(args["brief"]["look"], "pencil");
        assert_eq!(args["brief"]["palette"], "pencilMinimal");
    }
}
