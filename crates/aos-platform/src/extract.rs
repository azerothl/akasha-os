//! Extraction de faits chat → mémoire long terme (E14 / Preview 0.5).
//!
//! Filtre déterministe des secrets + parse JSON du modèle. La persistance
//! passe par [`crate::memory::MemoryStore::episodic_write_auto_link`].

use aos_proto::{MemExtractedFact, MemExtractOutcome, MemExtractOutcomeKind};
use serde::Deserialize;

/// Seuil cosinus au-delà duquel un fait est considéré comme doublon exact
/// (skip write ; l'auto-link `updates`/`supersedes` utilise 0.82).
pub const DEDUP_THRESHOLD: f32 = 0.92;

/// Parse la réponse modèle en liste de faits (tolère fence markdown).
pub fn parse_extract_json(raw: &str) -> Result<Vec<MemExtractedFact>, String> {
    let trimmed = strip_json_fence(raw.trim());
    #[derive(Deserialize)]
    struct Envelope {
        #[serde(default)]
        facts: Vec<FactIn>,
    }
    #[derive(Deserialize)]
    struct FactIn {
        text: String,
        #[serde(default)]
        supersedes_hint: Option<String>,
    }
    let env: Envelope = serde_json::from_str(trimmed)
        .or_else(|_| {
            // Parfois le modèle renvoie un tableau nu.
            let arr: Vec<FactIn> = serde_json::from_str(trimmed)?;
            Ok::<_, serde_json::Error>(Envelope { facts: arr })
        })
        .map_err(|e| format!("JSON extract invalide: {e}"))?;
    let mut out = Vec::new();
    for f in env.facts.into_iter().take(5) {
        let text = f.text.trim().to_string();
        if text.is_empty() {
            continue;
        }
        out.push(MemExtractedFact {
            text,
            supersedes_hint: f
                .supersedes_hint
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
        });
    }
    Ok(out)
}

fn strip_json_fence(s: &str) -> &str {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("```json") {
        return rest
            .strip_suffix("```")
            .unwrap_or(rest)
            .trim();
    }
    if let Some(rest) = s.strip_prefix("```") {
        return rest
            .strip_suffix("```")
            .unwrap_or(rest)
            .trim();
    }
    // Cherche le premier `{` / `[` si prose autour.
    if let Some(i) = s.find('{') {
        if let Some(j) = s.rfind('}') {
            if j > i {
                return &s[i..=j];
            }
        }
    }
    if let Some(i) = s.find('[') {
        if let Some(j) = s.rfind(']') {
            if j > i {
                return &s[i..=j];
            }
        }
    }
    s
}

/// True si le texte ressemble à un secret / identifiant (heuristique).
pub fn looks_like_secret(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    // Motifs étiquetés.
    const LABELS: &[&str] = &[
        "api_key",
        "api-key",
        "apikey",
        "password",
        "passwd",
        "secret",
        "bearer",
        "private key",
        "clé api",
        "mot de passe",
    ];
    for lab in LABELS {
        if lower.contains(lab) {
            return true;
        }
    }
    // Préfixes bien connus.
    if text.contains("sk-")
        || text.contains("ghp_")
        || text.contains("gho_")
        || text.contains("BSA")
        || lower.contains("begin private key")
        || lower.contains("-----begin")
    {
        return true;
    }
    // Token isolé haute entropie (longueur ≥ 20, alphanum + -_).
    for tok in text.split(|c: char| c.is_whitespace() || c == ',' || c == ';' || c == '"') {
        if looks_like_high_entropy_token(tok) {
            return true;
        }
    }
    // IBAN-like : 2 lettres + 2 digits + ≥10 alphanum.
    if iban_like(text) {
        return true;
    }
    false
}

fn looks_like_high_entropy_token(tok: &str) -> bool {
    let t = tok.trim();
    if t.len() < 20 {
        return false;
    }
    if !t
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return false;
    }
    // Doit contenir à la fois des lettres et des chiffres (évite phrases longues).
    let has_alpha = t.chars().any(|c| c.is_ascii_alphabetic());
    let has_digit = t.chars().any(|c| c.is_ascii_digit());
    if !(has_alpha && has_digit) {
        return false;
    }
    // Entropie approximative : ≥ 3 classes de caractères ou longueur ≥ 32.
    let mut classes = 0u8;
    if t.chars().any(|c| c.is_ascii_lowercase()) {
        classes += 1;
    }
    if t.chars().any(|c| c.is_ascii_uppercase()) {
        classes += 1;
    }
    if t.chars().any(|c| c.is_ascii_digit()) {
        classes += 1;
    }
    if t.chars().any(|c| c == '-' || c == '_' || c == '.') {
        classes += 1;
    }
    classes >= 3 || t.len() >= 32
}

fn iban_like(text: &str) -> bool {
    let compact: String = text
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_ascii_uppercase();
    let chars: Vec<char> = compact.chars().collect();
    if chars.len() < 15 {
        return false;
    }
    for window_start in 0..=chars.len().saturating_sub(15) {
        let slice = &chars[window_start..];
        if slice.len() >= 15
            && slice[0].is_ascii_alphabetic()
            && slice[1].is_ascii_alphabetic()
            && slice[2].is_ascii_digit()
            && slice[3].is_ascii_digit()
            && slice[4..].iter().take(11).all(|c| c.is_ascii_alphanumeric())
        {
            // Exige au moins 15 caractères de type IBAN (évite les faux positifs courts).
            return true;
        }
    }
    false
}

/// Classe chaque candidat : secret / vide / ok (à persist côté handler).
pub fn classify_candidates(facts: &[MemExtractedFact]) -> Vec<MemExtractOutcome> {
    facts
        .iter()
        .map(|f| {
            let text = f.text.trim().to_string();
            if text.is_empty() {
                MemExtractOutcome {
                    kind: MemExtractOutcomeKind::SkippedEmpty,
                    text,
                    id: None,
                    auto_relations: vec![],
                }
            } else if looks_like_secret(&text) {
                MemExtractOutcome {
                    kind: MemExtractOutcomeKind::FilteredSecret,
                    text,
                    id: None,
                    auto_relations: vec![],
                }
            } else {
                // Placeholder — le handler remplace par Stored / SkippedDuplicate.
                MemExtractOutcome {
                    kind: MemExtractOutcomeKind::Stored,
                    text,
                    id: None,
                    auto_relations: vec![],
                }
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_plain_json() {
        let raw = r#"{"facts":[{"text":"L'utilisateur préfère le français"}]}"#;
        let facts = parse_extract_json(raw).unwrap();
        assert_eq!(facts.len(), 1);
        assert!(facts[0].text.contains("français"));
    }

    #[test]
    fn parse_fenced_and_prose() {
        let raw = "Voici:\n```json\n{\"facts\":[{\"text\":\"prefère anglais\"}]}\n```\n";
        assert_eq!(parse_extract_json(raw).unwrap().len(), 1);
        let raw2 = "blabla {\"facts\":[]} suite";
        assert!(parse_extract_json(raw2).unwrap().is_empty());
    }

    #[test]
    fn parse_max_five() {
        let facts_json: String = (0..8)
            .map(|i| format!(r#"{{"text":"fait {i}"}}"#))
            .collect::<Vec<_>>()
            .join(",");
        let raw = format!(r#"{{"facts":[{facts_json}]}}"#);
        assert_eq!(parse_extract_json(&raw).unwrap().len(), 5);
    }

    #[test]
    fn filter_api_key_patterns() {
        assert!(looks_like_secret("my api_key is secret"));
        assert!(looks_like_secret("password: hunter2"));
        assert!(looks_like_secret("use sk-abcdefghijklmnopqrstuvwxyz1234"));
        assert!(looks_like_secret("token ghp_abcdefghijklmnopqrstuvwx"));
        assert!(looks_like_secret("BSA1234567890abcdefghijklmnop"));
        assert!(looks_like_secret("-----BEGIN PRIVATE KEY-----"));
        assert!(looks_like_secret("IBAN FR76 3000 6000 0112 3456 7890 189"));
        assert!(!looks_like_secret("L'utilisateur préfère le français"));
        assert!(!looks_like_secret("Je m'appelle Alice"));
    }

    #[test]
    fn classify_filters_secrets() {
        let facts = vec![
            MemExtractedFact {
                text: "L'utilisateur préfère le français".into(),
                supersedes_hint: None,
            },
            MemExtractedFact {
                text: "api_key sk-abcdefghijklmnopqrstuvwxyz".into(),
                supersedes_hint: None,
            },
        ];
        let out = classify_candidates(&facts);
        assert_eq!(out[0].kind, MemExtractOutcomeKind::Stored);
        assert_eq!(out[1].kind, MemExtractOutcomeKind::FilteredSecret);
    }
}
