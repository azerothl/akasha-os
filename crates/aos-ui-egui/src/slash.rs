//! Chat slash-command completions.

pub(crate) const SLASH_COMMANDS: &[(&str, &str)] = &[
    ("<texte>", "discuter avec l'assistant (modèle local)"),
    ("/commands", "cette liste"),
    ("/help", "état du système (services, agents, modèles)"),
    ("/agent <tâche>", "lancer un agent en fond (carte dans le chat)"),
    ("/notes", "lister les notes"),
    ("/notenew <titre> | <contenu>", "créer une note"),
    ("/notesearch <requête>", "recherche sémantique dans les notes"),
    ("/audit [n]", "n derniers événements d'audit"),
    ("/kill <id>", "tuer un agent"),
    ("/pause <id>", "suspendre un agent"),
    ("/image <prompt>", "generate a PNG under /downloads (uses Settings defaults)"),
    ("/speak <texte>", "open an in-chat TTS card (voice + knobs, then Generate)"),
    ("/canvas", "toggle the shared drawing canvas for this session"),
];

pub(crate) fn slash_completions(prefix: &str) -> Vec<(&'static str, &'static str)> {
    if !prefix.starts_with('/') {
        return Vec::new();
    }
    let token = prefix.split_whitespace().next().unwrap_or(prefix);
    SLASH_COMMANDS
        .iter()
        .copied()
        .filter(|(cmd, _)| {
            !cmd.starts_with('<') && cmd.split_whitespace().next().unwrap_or(cmd).starts_with(token)
        })
        .collect()
}

pub(crate) fn slash_insert_text(cmd_pattern: &str) -> String {
    let base = cmd_pattern.split_whitespace().next().unwrap_or(cmd_pattern);
    format!("{base} ")
}
