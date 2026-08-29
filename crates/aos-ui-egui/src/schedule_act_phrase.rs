//! Localized act-gate sentences for chat schedule creation.

use aos_agent::schedule_parse::ParsedSchedule;

use crate::i18n::UiStrings;

fn subst(template: &str, key: &str, value: &str) -> String {
    template.replace(&format!("{{{key}}}"), value)
}

/// Pending act sentence (locked EN/FR copy).
pub fn format_act_phrase(t: &UiStrings, goal: &str, when_label: &str, lang: &str) -> String {
    if lang.eq_ignore_ascii_case("fr") {
        let with_but = subst(t.schedule_act_phrase, "but", goal.trim());
        subst(&with_but, "quand", when_label.trim())
    } else {
        let with_goal = subst(t.schedule_act_phrase, "goal", goal.trim());
        subst(&with_goal, "when", when_label.trim())
    }
}

/// Resolved act prefix + detail.
pub fn format_resolved_act(
    t: &UiStrings,
    goal: &str,
    when_label: &str,
    approved: bool,
    lang: &str,
) -> String {
    let detail = format_act_phrase(t, goal, when_label, lang);
    let prefix = if approved {
        t.agent_act_resolved_approved
    } else {
        t.agent_act_resolved_denied
    };
    format!("{prefix} — {detail}")
}

pub fn act_phrase_from_parsed(t: &UiStrings, parsed: &ParsedSchedule, lang: &str) -> String {
    format_act_phrase(t, &parsed.goal, &parsed.when_label, lang)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aos_agent::schedule_parse;

    #[test]
    fn en_act_copy_keys() {
        let t = crate::i18n::strings("en");
        let parsed = schedule_parse::try_parse_phrase(
            "every morning, summarize my notes",
            schedule_parse::now_ms(),
            0,
        )
        .unwrap();
        let phrase = act_phrase_from_parsed(&t, &parsed, "en");
        assert_eq!(
            phrase,
            "Schedule: summarize my notes, every morning."
        );
        assert!(!phrase.contains("interval"));
        assert!(!phrase.contains("86400"));
        assert!(!phrase.contains("E2"));
    }

    #[test]
    fn fr_act_copy_keys() {
        let t = crate::i18n::strings("fr");
        let parsed = schedule_parse::try_parse_phrase(
            "chaque matin, résume mes notes",
            schedule_parse::now_ms(),
            0,
        )
        .unwrap();
        let phrase = act_phrase_from_parsed(&t, &parsed, "fr");
        assert_eq!(
            phrase,
            "Planifier : résume mes notes, chaque matin."
        );
        assert_ne!(phrase, crate::i18n::strings("en").schedule_act_phrase);
    }

    #[test]
    fn resolved_approved_en_fr() {
        let t_en = crate::i18n::strings("en");
        let t_fr = crate::i18n::strings("fr");
        let en = format_resolved_act(&t_en, "goal", "every morning", true, "en");
        let fr = format_resolved_act(&t_fr, "but", "chaque matin", true, "fr");
        assert!(en.starts_with("Allowed once"));
        assert!(fr.starts_with("Autorisé une fois"));
        assert_ne!(en, fr);
    }
}
