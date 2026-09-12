//! Compile-time inventory: chrome sources must not use emoji / tofu-prone glyphs as controls.
//!
//! Create module audit (Loïc #175): `modules/create/ui/index.json` ships label keys only;
//! interactive chrome is host-rendered in `rich_composition_ui.rs` (painted layer icons +
//! text i18n buttons). No font-symbol icon controls remain in the Create surface.

/// Rust sources scanned for control emoji (buttons, selectable labels used as icons).
const CHROME_SOURCES: &[&str] = &[
    include_str!("main.rs"),
    include_str!("ui_audit.rs"),
    include_str!("ui_chat_sidebar.rs"),
    include_str!("ui_primitives.rs"),
    include_str!("rich_composition_ui.rs"),
    include_str!("icons.rs"),
    include_str!("i18n.rs"),
];

/// Emoji and special symbols that must not appear in chrome control strings.
const FORBIDDEN_CONTROL_CHARS: &[char] = &[
    '🔘', '⛃', '🖼', '🗀', '☰', '🔔', '👁', '★', '✓', '×', '▲', '▼', '⇅', '▸', '▾', '☐', '☑', '□', '⋯',
];

fn line_looks_like_control(line: &str) -> bool {
    let t = line.trim();
    if t.starts_with("//") || t.starts_with("///") || t.starts_with('*') {
        return false;
    }
    t.contains("Button::new")
        || t.contains("small_button(")
        || t.contains("menu_button(")
        || t.contains("SelectableLabel::new")
        || t.contains(".button(")
}

fn find_control_emoji_violations(src: &str, file: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (i, line) in src.lines().enumerate() {
        if !line_looks_like_control(line) {
            continue;
        }
        for ch in FORBIDDEN_CONTROL_CHARS {
            if line.contains(*ch) {
                out.push(format!("{file}:{}: forbidden control glyph `{ch}`", i + 1));
            }
        }
        if line.contains("format!(\"🔔") || line.contains("'🔔") {
            out.push(format!("{file}:{}: notification bell emoji", i + 1));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_module_ui_json_has_no_control_font_glyphs() {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let candidates = [
            manifest.join("../../modules/create/ui/index.json"),
            manifest.join("../../share/modules/create.aospkg/ui/index.json"),
        ];
        let mut checked = false;
        for path in candidates {
            if !path.exists() {
                continue;
            }
            checked = true;
            let raw = std::fs::read_to_string(&path).expect("read create ui json");
            for ch in FORBIDDEN_CONTROL_CHARS {
                assert!(
                    !raw.contains(*ch),
                    "{}: forbidden control glyph `{}`",
                    path.display(),
                    ch
                );
            }
        }
        assert!(checked, "create ui index.json not found for audit");
    }

    #[test]
    fn chrome_inventory_has_no_control_emoji() {
        let files = [
            ("main.rs", CHROME_SOURCES[0]),
            ("ui_audit.rs", CHROME_SOURCES[1]),
            ("ui_chat_sidebar.rs", CHROME_SOURCES[2]),
            ("ui_primitives.rs", CHROME_SOURCES[3]),
            ("rich_composition_ui.rs", CHROME_SOURCES[4]),
            ("icons.rs", CHROME_SOURCES[5]),
            ("i18n.rs", CHROME_SOURCES[6]),
        ];
        let mut violations = Vec::new();
        for (name, src) in files {
            violations.extend(find_control_emoji_violations(src, name));
        }
        assert!(
            violations.is_empty(),
            "chrome control emoji inventory failed:\n{}",
            violations.join("\n")
        );
    }
}
