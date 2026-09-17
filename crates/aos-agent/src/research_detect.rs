//! Heuristics for research-shaped chat questions (document choice card).

/// User already asked for a prepared document — skip the choice card /
/// prefer `files.generate` over notes.
pub fn user_requested_document(text: &str) -> bool {
    let lower = text.to_lowercase();
    const MARKERS: &[&str] = &[
        // EN
        "prepare a document",
        "prepare document",
        "write a document",
        "write me a document",
        "make a document",
        "make me a document",
        "generate a document",
        "create a document",
        "document about",
        "research document",
        "write a report",
        "prepare a report",
        "write a presentation",
        "presentation document",
        "document presentation",
        // FR — infinitives / conjugations / spacing variants
        "préparer un document",
        "prépare un document",
        "prépare-moi un document",
        "prépare moi un document",
        "preparer un document",
        "prepare un document",
        "rédiger un document",
        "rediger un document",
        "rédige un document",
        "redige un document",
        "rédige-moi un document",
        "rédige moi un document",
        "redige-moi un document",
        "redige moi un document",
        "écrire un document",
        "ecrire un document",
        "écris un document",
        "ecris un document",
        "écris-moi un document",
        "écris moi un document",
        "génère un document",
        "genere un document",
        "génère-moi un document",
        "genere moi un document",
        "crée un document",
        "cree un document",
        "créer un document",
        "creer un document",
        "fais un document",
        "fais-moi un document",
        "fais moi un document",
        "fait moi un document",
        "faites-moi un document",
        "faites moi un document",
        "faire un document",
        "document sur",
        "document de présentation",
        "document de presentation",
        "prépare un rapport",
        "prepare un rapport",
        "rédige un rapport",
        "redige un rapport",
        "fais un rapport",
        "fais-moi un rapport",
        "fais moi un rapport",
    ];
    MARKERS.iter().any(|m| lower.contains(m))
}

/// User asked for an internal note / carnet entry (not a `/downloads/` document file).
pub fn user_requested_note(text: &str) -> bool {
    let lower = text.to_lowercase();
    const MARKERS: &[&str] = &[
        "write a note",
        "make a note",
        "create a note",
        "add a note",
        "note about",
        "quick note",
        "écris une note",
        "ecris une note",
        "écris-moi une note",
        "ecris-moi une note",
        "crée une note",
        "cree une note",
        "créer une note",
        "creer une note",
        "fais une note",
        "fais-moi une note",
        "fais moi une note",
        "note rapide",
        "dans le carnet",
        "au carnet",
    ];
    MARKERS.iter().any(|m| lower.contains(m))
}

/// User asked for live host machine specs (GPU/VRAM/RAM/disk) — not a sandbox path.
pub fn user_requested_hardware(text: &str) -> bool {
    let lower = text.to_lowercase();
    if lower.contains("system.hardware") {
        return true;
    }
    const MARKERS: &[&str] = &[
        // FR
        "état de la machine",
        "etat de la machine",
        "état machine",
        "etat machine",
        "specs de la machine",
        "spécifications de la machine",
        "specifications de la machine",
        "caractéristiques de la machine",
        "caracteristiques de la machine",
        "matériel de la machine",
        "materiel de la machine",
        "ma machine",
        "mon pc",
        "mon gpu",
        "ma vram",
        "combien de vram",
        "combien de ram",
        "snapshot matériel",
        "snapshot materiel",
        // EN
        "machine state",
        "host hardware",
        "hardware snapshot",
        "system specs",
        "machine specs",
        "how much vram",
        "how much ram",
        "my gpu",
        "my vram",
    ];
    if MARKERS.iter().any(|m| lower.contains(m)) {
        return true;
    }
    // Short asks: "VRAM ?", "GPU/RAM", "état GPU"
    let compact = lower.replace(['?', '!', '.'], " ");
    let tokens: Vec<&str> = compact.split_whitespace().collect();
    let has = |w: &str| tokens.contains(&w);
    (has("vram") || has("gpu") || has("ram"))
        && (has("état")
            || has("etat")
            || has("state")
            || has("specs")
            || has("combien")
            || has("how")
            || has("quelle")
            || has("quel")
            || has("dispo")
            || has("disponible")
            || has("libre")
            || has("free"))
}

fn summary_str(summary: &serde_json::Value, key: &str) -> Option<String> {
    summary.get(key).and_then(|v| match v {
        serde_json::Value::String(s) if !s.is_empty() => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        _ => None,
    })
}

fn mib_to_gib_label(mib: u64) -> String {
    let gib = (mib as f64) / 1024.0;
    if gib >= 10.0 {
        format!("{:.0} Go ({mib} MiB)", gib)
    } else {
        format!("{:.1} Go ({mib} MiB)", gib)
    }
}

/// Human-readable machine report from a `system.hardware` summary JSON.
/// Prefer this over asking a small chat model to paraphrase the snapshot.
pub fn format_hardware_user_reply(summary: &serde_json::Value) -> String {
    let os = summary_str(summary, "os").unwrap_or_else(|| "?".into());
    let arch = summary_str(summary, "arch").unwrap_or_else(|| "?".into());
    let gpu = summary_str(summary, "gpu_name").unwrap_or_else(|| "inconnu".into());
    let tier = summary_str(summary, "tier").unwrap_or_else(|| "?".into());
    let driver = summary_str(summary, "driver_version").unwrap_or_default();
    let vram_mib = summary
        .get("vram_mib")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let vram_used = summary.get("vram_used_mib").and_then(|v| v.as_u64());
    let vram_free = summary.get("vram_free_mib").and_then(|v| v.as_u64());
    let ram_mib = summary.get("ram_mib").and_then(|v| v.as_u64()).unwrap_or(0);
    let disk = summary
        .get("disk_free_gib")
        .and_then(|v| v.as_f64())
        .map(|g| format!("{g:.1} Go libres"))
        .unwrap_or_else(|| "?".into());

    let mut lines = vec![
        "**État machine (snapshot frais)**".to_string(),
        format!("- **OS / arch** : {os} ({arch})"),
        format!("- **GPU** : {gpu}"),
        format!("- **VRAM** : {}", mib_to_gib_label(vram_mib)),
    ];
    match (vram_used, vram_free) {
        (Some(u), Some(f)) => {
            lines.push(format!(
                "- **VRAM utilisée / libre** : {} / {}",
                mib_to_gib_label(u),
                mib_to_gib_label(f)
            ));
        }
        (None, Some(f)) => {
            lines.push(format!("- **VRAM libre** : {}", mib_to_gib_label(f)));
        }
        (Some(u), None) => {
            lines.push(format!("- **VRAM utilisée** : {}", mib_to_gib_label(u)));
        }
        _ => {}
    }
    lines.push(format!("- **RAM** : {}", mib_to_gib_label(ram_mib)));
    lines.push(format!("- **Disque (AOS_HOME)** : {disk}"));
    lines.push(format!("- **Tier placement** : {tier}"));
    if !driver.is_empty() {
        lines.push(format!("- **Driver GPU** : {driver}"));
    }
    if let Some(thermal) = summary.get("thermal") {
        let temp = thermal
            .get("temperature_c")
            .and_then(|v| v.as_f64())
            .map(|t| format!("{t:.0} °C"));
        let power = thermal
            .get("power_w")
            .and_then(|v| v.as_f64())
            .map(|p| format!("{p:.0} W"));
        let throttle = thermal
            .get("throttling")
            .and_then(|v| v.as_bool())
            .map(|b| if b { "oui" } else { "non" });
        let parts: Vec<String> = [temp, power, throttle.map(|t| format!("throttling: {t}"))]
            .into_iter()
            .flatten()
            .collect();
        if !parts.is_empty() {
            lines.push(format!("- **Thermique** : {}", parts.join(" · ")));
        }
    }
    if let Some(npu) = summary.get("npu") {
        if let Some(name) = npu.get("name").and_then(|v| v.as_str()) {
            lines.push(format!("- **NPU** : {name}"));
        }
    }
    lines.join("\n")
}

/// Prompt block so salon agents answer from a fresh probe without inventing gaps.
pub fn format_hardware_context_block(summary: &serde_json::Value) -> String {
    let prose = format_hardware_user_reply(summary);
    let body = serde_json::to_string_pretty(summary).unwrap_or_else(|_| summary.to_string());
    format!(
        "\n## Snapshot machine (frais, system.hardware)\n{prose}\n\nJSON:\n{body}\n\
         Reproduis **tous** les champs ci-dessus (GPU, VRAM totale/utilisée/libre, RAM, disque, tier, thermique). \
         N'affirme jamais que tu n'as pas accès au matériel ni que `system.hardware` est indisponible.\n"
    )
}

/// Document or note delivery that should invoke salon tools — not prose-only talk.
pub fn user_implies_room_tool_action(text: &str) -> bool {
    user_requested_document(text) || user_requested_note(text) || user_requested_hardware(text)
}

/// Ensure `file-author` + `files.generate` so a document ask can land under `/downloads/`.
pub fn ensure_document_file_tools(skills: &mut Vec<String>, tool_ids: &mut Vec<String>) {
    if !skills.iter().any(|s| s == "file-author") {
        skills.push("file-author".into());
    }
    if !tool_ids.iter().any(|t| t == "files.generate") {
        tool_ids.push("files.generate".into());
    }
}

/// True when the user message looks like an open research question worth offering
/// Reply vs Prepare a document — not slash commands, canvas, or short chit-chat.
pub fn is_research_shaped_ask(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.starts_with('/') {
        return false;
    }
    if trimmed.chars().count() < 12 {
        return false;
    }
    let lower = trimmed.to_lowercase();
    if chat_canvas::chat_user_wants_explicit_canvas(trimmed)
        || chat_canvas::chat_user_has_draw_wording(trimmed)
        || lower.contains("dessine")
        || lower.contains("canvas")
    {
        return false;
    }
    if lower.contains("module")
        || lower.contains("aospkg")
        || lower.contains("scaffold")
        || lower.contains("/speak")
        || lower.contains("tts")
    {
        return false;
    }
    let question_mark = trimmed.ends_with('?')
        || trimmed.ends_with('？')
        || lower.starts_with("what ")
        || lower.starts_with("how ")
        || lower.starts_with("why ")
        || lower.starts_with("which ")
        || lower.starts_with("when ")
        || lower.starts_with("where ")
        || lower.starts_with("who ")
        || lower.starts_with("quel ")
        || lower.starts_with("quelle ")
        || lower.starts_with("quels ")
        || lower.starts_with("quelles ")
        || lower.starts_with("comment ")
        || lower.starts_with("pourquoi ")
        || lower.starts_with("quand ")
        || lower.starts_with("où ")
        || lower.starts_with("ou ")
        || lower.contains("qu'est-ce")
        || lower.contains("qu’est-ce");
    if !question_mark {
        return false;
    }
    const RESEARCH_MARKERS: &[&str] = &[
        "state of the art",
        "state-of-the-art",
        "état de l'art",
        "etat de l'art",
        "état de l’art",
        "research",
        "recherche",
        "survey",
        "overview",
        "landscape",
        "compare",
        "comparison",
        "comparer",
        "tendance",
        "trends",
        "literature",
        "littérature",
        "literature review",
        "revue",
        "agentic",
        "agents",
        "best practice",
        "bonnes pratiques",
        "state of",
        "sota",
        "recent",
        "récent",
        "current",
        "actuel",
        "applications",
        "ecosystem",
        "écosystème",
    ];
    RESEARCH_MARKERS.iter().any(|m| lower.contains(m))
}

/// System prompt slice for document-prep agents (research + file-author skills).
pub fn document_prep_system_prompt(language: &str) -> String {
    let fr = language.eq_ignore_ascii_case("fr");
    if fr {
        "Tu rédiges toi-même un document structuré pour l'utilisateur (fichier sous /downloads/), \
         tu ne cherches pas un document déjà existant.\n\
         La directive contient une section « Contexte du fil » : résous les références \
         (« ce type de module », « ça », « ce dont on a parlé ») à partir de ce contexte. \
         Ne pars pas en hors-sujet web si le fil parle déjà d'un sujet produit concret.\n\
         Ordre : (1) memory.recall / contexte produit si pertinent, \
         (2) web.search + web.browse seulement si des faits externes manquent \
         (tu peux paralléliser des recherches via agent.spawn + agent.await), \
         (3) files.generate en markdown sous /downloads/.\n\
         Forme : titre, courte introduction, sections, notes de bas de page numérotées. \
         Chaque fait web DOIT avoir une note (titre + URL). N'invente jamais de source. \
         Si un fetch échoue, dis-le dans le document.\n\
         Images seulement si utile via media.image.generate. Pas de diaporama.\n\
         Le fichier est ajouté automatiquement à la Bibliothèque.\n\
         Termine avec goal.complete en citant le chemin du fichier produit."
            .into()
    } else {
        "You write a structured document yourself for the user (file under /downloads/); \
         you are not looking up an existing document to download.\n\
         The directive includes a « Thread context » section: resolve references \
         (« this kind of module », « that », « what we discussed ») from that context. \
         Do not drift into unrelated web topics when the thread already names a concrete subject.\n\
         Order: (1) memory.recall / product context when relevant, \
         (2) web.search + web.browse only when external facts are missing \
         (you may parallelize research via agent.spawn + agent.await), \
         (3) files.generate as markdown under /downloads/.\n\
         Shape: title, short lede, sections, numbered footnotes. \
         Every web fact MUST have a footnote (title + URL). Never invent a source. \
         If a fetch fails, say so in the document.\n\
         Images only when useful via media.image.generate. No slide deck.\n\
         The file is automatically added to the user Library.\n\
         Finish with goal.complete citing the output file path."
            .into()
    }
}

const DOCUMENT_PREP_CONTEXT_MAX_CHARS: usize = 6_000;
const DOCUMENT_PREP_CONTEXT_MAX_TURNS: usize = 12;
const DOCUMENT_PREP_MSG_MAX_CHARS: usize = 800;

/// Goal/directive for document-prep: user ask + recent chat so deictic asks stay grounded.
pub fn document_prep_goal_from_thread(
    history: &[(impl AsRef<str>, impl AsRef<str>)],
    question: &str,
) -> String {
    let question = question.trim();
    let mut prior: Vec<(String, String)> = Vec::new();
    for (role, content) in history {
        let role = normalize_thread_role(role.as_ref());
        let content = content.as_ref().trim();
        if content.is_empty() {
            continue;
        }
        prior.push((
            role.into(),
            clip_chars(content, DOCUMENT_PREP_MSG_MAX_CHARS),
        ));
    }
    // Drop trailing duplicate of the current question (history usually includes it).
    if let Some((_, last)) = prior.last() {
        if last.trim() == question {
            prior.pop();
        }
    }
    if prior.len() > DOCUMENT_PREP_CONTEXT_MAX_TURNS {
        let skip = prior.len() - DOCUMENT_PREP_CONTEXT_MAX_TURNS;
        prior = prior.into_iter().skip(skip).collect();
    }

    if prior.is_empty() {
        return question.to_string();
    }

    let mut ctx = String::from("Thread context (resolve references from here):\n");
    for (role, content) in &prior {
        ctx.push_str(role);
        ctx.push_str(": ");
        ctx.push_str(content);
        ctx.push('\n');
    }
    let mut out = format!("User request:\n{question}\n\n{ctx}");
    if out.chars().count() > DOCUMENT_PREP_CONTEXT_MAX_CHARS {
        out = clip_chars(&out, DOCUMENT_PREP_CONTEXT_MAX_CHARS);
    }
    out
}

fn normalize_thread_role(role: &str) -> &'static str {
    match role.trim().to_ascii_lowercase().as_str() {
        "assistant" | "system" => "assistant",
        _ => "user",
    }
}

fn clip_chars(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        return s.to_string();
    }
    format!(
        "{}…",
        s.chars().take(max.saturating_sub(1)).collect::<String>()
    )
}

// Re-use canvas heuristics from the UI crate via duplicated thin checks to keep agent crate independent.
mod chat_canvas {
    pub fn chat_user_wants_explicit_canvas(text: &str) -> bool {
        let lower = text.to_lowercase();
        lower.contains("canvas") || lower.contains("dessine sur le canvas")
    }

    pub fn chat_user_has_draw_wording(text: &str) -> bool {
        let lower = text.to_lowercase();
        ["dessine", "draw ", "sketch", "trace "]
            .iter()
            .any(|w| lower.contains(w))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn research_shaped_en() {
        assert!(is_research_shaped_ask(
            "what is the state of the art of agentic apps?"
        ));
        assert!(is_research_shaped_ask(
            "How do current agentic application frameworks compare?"
        ));
    }

    #[test]
    fn research_shaped_fr() {
        assert!(is_research_shaped_ask(
            "quel est l'état de l'art des applications agentic ?"
        ));
    }

    #[test]
    fn not_research_short_or_slash() {
        assert!(!is_research_shaped_ask("hi"));
        assert!(!is_research_shaped_ask("/image a cat"));
        assert!(!is_research_shaped_ask("thanks!"));
    }

    #[test]
    fn not_research_canvas_or_module() {
        assert!(!is_research_shaped_ask("dessine une maison sur le canvas"));
        assert!(!is_research_shaped_ask("create a module ping?"));
    }

    #[test]
    fn plain_question_without_research_markers() {
        assert!(!is_research_shaped_ask("what time is it?"));
    }

    #[test]
    fn user_requested_document_skips_choice() {
        assert!(user_requested_document(
            "Please prepare a document about agentic apps"
        ));
        assert!(user_requested_document(
            "Peux-tu préparer un document sur l'état de l'art ?"
        ));
        assert!(user_requested_document(
            "fais moi un document de présentation de ce dont on a parlé"
        ));
        assert!(user_requested_document("rédige moi un document là-dessus"));
        assert!(!user_requested_document("what is the state of the art?"));
        assert!(!user_requested_document("écris une note rapide"));
        assert!(user_requested_note("écris une note rapide"));
        assert!(user_implies_room_tool_action("écris une note rapide"));
        assert!(user_implies_room_tool_action(
            "prepare a document about rust"
        ));
        assert!(user_requested_hardware("donne moi l'etat de la machine"));
        assert!(user_requested_hardware("re essai avec l'outil system.hardware"));
        assert!(user_requested_hardware("how much VRAM do I have?"));
        assert!(!user_requested_hardware("what is the state of the art?"));
        assert!(user_implies_room_tool_action("état de la machine"));
        let reply = format_hardware_user_reply(&serde_json::json!({
            "os": "windows",
            "arch": "x86_64",
            "gpu_name": "NVIDIA GeForce RTX 4080 SUPER",
            "vram_mib": 16376,
            "vram_used_mib": 10000,
            "vram_free_mib": 6000,
            "ram_mib": 63092,
            "disk_free_gib": 120.5,
            "tier": "mid",
            "driver_version": "560.94",
            "thermal": {"temperature_c": 52.0, "power_w": 80.0, "throttling": false}
        }));
        assert!(reply.contains("RTX 4080 SUPER"), "{reply}");
        assert!(reply.contains("VRAM"), "{reply}");
        assert!(reply.contains("RAM"), "{reply}");
        assert!(reply.contains("120.5"), "{reply}");
        assert!(reply.contains("52"), "{reply}");
    }

    #[test]
    fn ensure_document_file_tools_idempotent() {
        let mut skills = vec!["planner".into()];
        let mut tools = vec!["notes.create".into()];
        ensure_document_file_tools(&mut skills, &mut tools);
        ensure_document_file_tools(&mut skills, &mut tools);
        assert_eq!(skills.iter().filter(|s| *s == "file-author").count(), 1);
        assert_eq!(tools.iter().filter(|t| *t == "files.generate").count(), 1);
    }

    #[test]
    fn document_prep_goal_includes_prior_thread() {
        let history = vec![
            ("user", "comment créer un module d'aide au développement ?"),
            ("assistant", "Utilise le catalogue : scaffold, package, install."),
            (
                "user",
                "est ce que tu peux me créer un document de specs détaillé pour ce type de module ?",
            ),
        ];
        let q =
            "est ce que tu peux me créer un document de specs détaillé pour ce type de module ?";
        let goal = document_prep_goal_from_thread(&history, q);
        assert!(goal.contains("User request:"), "{goal}");
        assert!(goal.contains("Thread context"), "{goal}");
        assert!(goal.contains("aide au développement"), "{goal}");
        assert!(goal.contains("scaffold"), "{goal}");
        assert!(
            goal.matches(q).count() >= 1,
            "question should appear once in request: {goal}"
        );
        // Trailing duplicate of q dropped from context block
        let after_ctx = goal.split("Thread context").nth(1).unwrap_or("");
        assert!(
            !after_ctx.contains("document de specs"),
            "should not repeat the ask inside context: {goal}"
        );
    }

    #[test]
    fn document_prep_goal_without_history_is_question() {
        let goal = document_prep_goal_from_thread(&[] as &[(&str, &str)], "Write a report on X");
        assert_eq!(goal, "Write a report on X");
    }
}
