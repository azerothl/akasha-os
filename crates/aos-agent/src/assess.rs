//! Estimation de complexité d'une tâche (`task.assess`).

use serde::{Deserialize, Serialize};

/// Résultat d'une évaluation de complexité.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssessResult {
    /// `"simple"` ou `"complex"`.
    pub complexity: String,
    pub reason: String,
    pub needs_plan: bool,
}

impl AssessResult {
    pub fn simple(reason: impl Into<String>) -> Self {
        Self {
            complexity: "simple".into(),
            reason: reason.into(),
            needs_plan: false,
        }
    }

    pub fn complex(reason: impl Into<String>) -> Self {
        Self {
            complexity: "complex".into(),
            reason: reason.into(),
            needs_plan: true,
        }
    }

    pub fn is_complex(&self) -> bool {
        self.needs_plan || self.complexity.eq_ignore_ascii_case("complex")
    }
}

#[derive(Debug, Deserialize)]
struct AssessJson {
    #[serde(default)]
    complexity: String,
    #[serde(default)]
    reason: String,
    #[serde(default)]
    needs_plan: Option<bool>,
}

/// Parse la sortie modèle de `task.assess`.
/// Fallback : `complex` si le JSON est illisible (sécurité → forcer un plan).
pub fn parse_assess_response(text: &str) -> AssessResult {
    let clean = crate::actions::strip_reasoning(text);
    if let Some(json) = extract_json(&clean) {
        // Some local models repeat a key while streaming JSON (for example
        // `"complexity":"simple","complexity":"simple"`). Deserializing
        // directly into a struct rejects that otherwise usable answer; parse
        // through Value so serde keeps the final equivalent value instead.
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&json) {
            if let Ok(raw) = serde_json::from_value::<AssessJson>(value) {
                return normalize_assess(raw);
            }
        }
    }
    AssessResult::complex("réponse task.assess illisible — plan requis par défaut")
}

fn normalize_assess(raw: AssessJson) -> AssessResult {
    let c = raw.complexity.trim().to_ascii_lowercase();
    let complex = matches!(c.as_str(), "complex" | "complexe" | "hard" | "reasoning")
        || raw.needs_plan == Some(true);
    let reason = if raw.reason.trim().is_empty() {
        if complex {
            "tâche jugée complexe".into()
        } else {
            "tâche jugée simple".into()
        }
    } else {
        raw.reason.trim().to_string()
    };
    if complex {
        AssessResult {
            complexity: "complex".into(),
            reason,
            needs_plan: true,
        }
    } else {
        AssessResult {
            complexity: "simple".into(),
            reason,
            needs_plan: false,
        }
    }
}

fn extract_json(text: &str) -> Option<String> {
    if let Some(start) = text.find("```json") {
        let after = &text[start + 7..];
        if let Some(end) = after.find("```") {
            return Some(after[..end].trim().to_string());
        }
    }
    let start = text.find('{')?;
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escape = false;
    for (i, ch) in text[start..].char_indices() {
        if in_str {
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                in_str = false;
            }
            continue;
        }
        match ch {
            '"' => in_str = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(text[start..start + i + 1].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_json() {
        let r = parse_assess_response(
            r#"{"complexity":"simple","reason":"une note","needs_plan":false}"#,
        );
        assert_eq!(r.complexity, "simple");
        assert!(!r.needs_plan);
        assert!(r.reason.contains("note"));
    }

    #[test]
    fn parse_complex_fence() {
        let r = parse_assess_response(
            r#"```json
{"complexity":"complex","reason":"plusieurs livrables","needs_plan":true}
```"#,
        );
        assert!(r.is_complex());
        assert_eq!(r.complexity, "complex");
    }

    #[test]
    fn parse_needs_plan_overrides_simple_label() {
        let r = parse_assess_response(
            r#"{"complexity":"simple","reason":"x","needs_plan":true}"#,
        );
        assert!(r.needs_plan);
        assert_eq!(r.complexity, "complex");
    }

    #[test]
    fn parse_fallback_on_garbage() {
        let r = parse_assess_response("je ne sais pas");
        assert!(r.is_complex());
        assert!(r.needs_plan);
    }

    #[test]
    fn parse_strips_think_tags() {
        let r = parse_assess_response(
            r#"<think>hmm</think>
{"complexity":"simple","reason":"ok","needs_plan":false}"#,
        );
        assert_eq!(r.complexity, "simple");
    }

    #[test]
    fn parse_accepts_a_repeated_equivalent_key_from_a_local_model() {
        let r = parse_assess_response(
            r#"{"complexity":"simple","complexity":"simple","reason":"un dessin","needs_plan":false}"#,
        );
        assert_eq!(r.complexity, "simple");
        assert!(!r.needs_plan);
    }
}
