//! Heuristiques partagées pour l'extraction mémoire (E14) — filtres déterministes
//! keep vs drop, utilisables par `aos-platformd` et la Preview UI.

/// True si le tour entier est une demande canvas / dessin sans signal durable.
pub fn should_skip_mem_extract_turn(user_text: &str) -> bool {
    let trimmed = user_text.trim();
    if trimmed.is_empty() {
        return true;
    }
    is_draw_or_canvas_request(trimmed) && !has_durable_fact_signals(trimmed)
}

/// True si le texte ressemble à une trace outil / log agent (pas un fait humain).
pub fn looks_like_tool_trace(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() {
        return false;
    }
    if t.starts_with('✓') {
        return true;
    }
    if t.contains('`') && (t.contains("→") || t.contains("->")) {
        return true;
    }
    let lower = t.to_ascii_lowercase();
    if lower.contains("[mem.context]")
        || lower.contains("tool.invoke")
        || lower.contains("agent.spawn")
        || lower.contains("canvas.stroke")
        || lower.contains("canvas.rect")
        || lower.contains("canvas.export")
        || lower.contains("media.image.generate")
    {
        return true;
    }
    if lower.contains("lance un agent pour dessiner")
        || lower.contains("launching an agent to draw")
        || lower.contains("launch an agent to draw")
    {
        return true;
    }
    if t.contains("\"action\"") && t.contains("\"args\"") {
        return true;
    }
    false
}

/// True si le fait extrait décrit une tâche éphémère (canvas, dessin, génération).
pub fn looks_like_ephemeral_fact(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    if looks_like_tool_trace(text) {
        return true;
    }
    if is_standing_preference(&lower) {
        return false;
    }
    if is_draw_or_canvas_request(text) {
        return true;
    }
    const EPHEMERAL_PATTERNS: &[&str] = &[
        "veut dessiner",
        "a demandé de dessiner",
        "souhaite dessiner",
        "demande de dessiner",
        "asked to draw",
        "asked to sketch",
        "wants to draw",
        "wants to sketch",
        "requested a drawing",
        "requested to draw",
        "on the canvas",
        "sur le canvas",
        "dans le canvas",
        "on the canevas",
        "sur le canevas",
        "au trait",
        "image generation",
        "génération d'image",
        "generate an image",
        "générer une image",
    ];
    EPHEMERAL_PATTERNS.iter().any(|p| lower.contains(p))
}

/// True si le texte doit apparaître dans l'onglet Mémoire (fait humain durable).
pub fn is_human_memory_fact(text: &str) -> bool {
    let t = text.trim();
    !t.is_empty() && !looks_like_tool_trace(t) && !looks_like_ephemeral_fact(t)
}

fn is_standing_preference(lower: &str) -> bool {
    const MARKERS: &[&str] = &[
        "préfère",
        "prefere",
        "prefers",
        "aime",
        "likes",
        "love ",
        "habituellement",
        "usually",
        "toujours",
        "always",
        "en général",
        "in general",
        "standing",
        "favori",
        "favorite",
        "favourite",
    ];
    MARKERS.iter().any(|m| lower.contains(m))
}

fn has_durable_fact_signals(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    const SIGNALS: &[&str] = &[
        "je m'appelle",
        "je m appelle",
        "my name is",
        "i'm ",
        "i am ",
        "je suis ",
        "j'habite",
        "j habite",
        "i live in",
        "i work",
        "je travaille",
        "préfère",
        "prefere",
        "prefers",
        "i like",
        "j'aime",
        "j aime",
        "i love",
        "toujours",
        "always",
        "habituellement",
        "usually",
        "mon email",
        "my email",
        "call me",
        "appelez-moi",
    ];
    SIGNALS.iter().any(|s| lower.contains(s)) || is_standing_preference(&lower)
}

fn word_boundary_match(lower: &str, pat: &str) -> bool {
    lower
        .split(|c: char| !c.is_alphanumeric())
        .any(|word| word == pat)
}

/// Demande de dessin / canvas (tour utilisateur ou fait extrait).
pub fn is_draw_or_canvas_request(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    const EXPLICIT_CANVAS: &[&str] = &[
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
    if EXPLICIT_CANVAS.iter().any(|m| lower.contains(m)) {
        return true;
    }
    const DRAW_WORDS: &[&str] = &[
        "dessin",
        "dessine",
        "dessiner",
        "draw",
        "drawing",
        "sketch",
        "trace",
        "tracer",
        "esquisse",
        "redessine",
        "redessiner",
        "illustration",
        "illustrer",
    ];
    DRAW_WORDS.iter().any(|k| word_boundary_match(&lower, k))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skip_pure_draw_turn() {
        assert!(should_skip_mem_extract_turn("dessine moi une maison"));
        assert!(should_skip_mem_extract_turn("draw a cat on the canvas"));
        assert!(!should_skip_mem_extract_turn(
            "Je m'appelle Alice et dessine une maison"
        ));
        assert!(!should_skip_mem_extract_turn("Je préfère le français"));
    }

    #[test]
    fn human_memory_fact_gate() {
        assert!(is_human_memory_fact("L'utilisateur s'appelle Alice"));
        assert!(is_human_memory_fact("L'utilisateur préfère le français"));
        assert!(!is_human_memory_fact("L'utilisateur veut dessiner une maison"));
        assert!(!is_human_memory_fact("✓ `notes.create` → `todo.md`"));
    }

    #[test]
    fn standing_draw_preference_is_kept() {
        assert!(!looks_like_ephemeral_fact(
            "L'utilisateur aime dessiner à l'aquarelle"
        ));
    }
}
