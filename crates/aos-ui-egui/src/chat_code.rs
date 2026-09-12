//! Fenced code blocks with syntect highlighting and a visible language chip.

use eframe::egui;
use std::sync::OnceLock;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;

fn syntax_set() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn theme_dark() -> &'static Theme {
    static THEME: OnceLock<Theme> = OnceLock::new();
    THEME.get_or_init(|| {
        let themes = ThemeSet::load_defaults();
        themes
            .themes
            .get("base16-ocean.dark")
            .or_else(|| themes.themes.get("Solarized (dark)"))
            .cloned()
            .or_else(|| themes.themes.values().next().cloned())
            .expect("syntect ships at least one theme")
    })
}

fn theme_light() -> &'static Theme {
    static THEME: OnceLock<Theme> = OnceLock::new();
    THEME.get_or_init(|| {
        let themes = ThemeSet::load_defaults();
        themes
            .themes
            .get("base16-ocean.light")
            .or_else(|| themes.themes.get("InspiredGitHub"))
            .cloned()
            .or_else(|| themes.themes.values().next().cloned())
            .expect("syntect ships at least one theme")
    })
}

/// Map model / CommonMark language tags to syntect tokens and display labels.
pub fn normalize_fence_lang(lang: &str) -> (String, String) {
    let raw = lang.trim().to_ascii_lowercase();
    let (token, label) = match raw.as_str() {
        "" => ("", "code"),
        "js" | "node" | "nodejs" | "javascript" | "react" => ("js", "javascript"),
        // Syntect's default pack has no TypeScript grammar; JS highlighting is close enough.
        "ts" | "typescript" | "tsx" => ("js", "typescript"),
        "jsx" => ("js", "jsx"),
        "rs" | "rust" => ("rs", "rust"),
        "py" | "python" => ("py", "python"),
        "sh" | "bash" | "zsh" | "shell" | "console" | "terminal" | "cmd" => ("sh", "bash"),
        "ps1" | "powershell" => ("ps1", "powershell"),
        "json" | "jsonc" | "json5" => ("json", "json"),
        "yml" | "yaml" => ("yaml", "yaml"),
        "ex" | "elixir" | "exs" => ("rb", "elixir"), // closest default grammar; chip keeps elixir
        "rb" | "ruby" => ("rb", "ruby"),
        "go" | "golang" => ("go", "go"),
        "cs" | "csharp" | "c#" => ("cs", "csharp"),
        "cpp" | "c++" | "cxx" | "cc" => ("cpp", "cpp"),
        "h" | "hpp" | "hh" => ("h", "c"),
        "c" => ("c", "c"),
        "java" => ("java", "java"),
        "kt" | "kotlin" => ("kt", "kotlin"),
        "swift" => ("swift", "swift"),
        "lua" => ("lua", "lua"),
        "php" => ("php", "php"),
        "sql" => ("sql", "sql"),
        "html" | "htm" => ("html", "html"),
        "css" | "scss" | "less" => ("css", "css"),
        "xml" => ("xml", "xml"),
        "toml" => ("toml", "toml"),
        "md" | "markdown" => ("md", "markdown"),
        "diff" | "patch" => ("diff", "diff"),
        "dockerfile" | "docker" => ("Dockerfile", "dockerfile"),
        "makefile" | "make" => ("Makefile", "makefile"),
        "wasm" | "wat" => ("wasm", "wasm"),
        "graphql" | "gql" => ("graphql", "graphql"),
        "proto" | "protobuf" => ("proto", "protobuf"),
        "vue" => ("vue", "vue"),
        "svelte" => ("svelte", "svelte"),
        "zig" => ("zig", "zig"),
        "nim" => ("nim", "nim"),
        "hs" | "haskell" => ("hs", "haskell"),
        "dart" => ("dart", "dart"),
        "text" | "plaintext" | "txt" | "plain" => ("txt", "text"),
        other => (other, other),
    };
    (token.to_string(), label.to_string())
}

fn find_syntax<'a>(ps: &'a SyntaxSet, token: &str) -> Option<&'a SyntaxReference> {
    if token.is_empty() {
        return None;
    }
    ps.find_syntax_by_token(token)
        .or_else(|| ps.find_syntax_by_extension(token))
        .or_else(|| ps.find_syntax_by_name(token))
        .or_else(|| match token {
            "ts" => ps
                .find_syntax_by_name("TypeScript")
                .or_else(|| ps.find_syntax_by_extension("ts")),
            "tsx" => ps
                .find_syntax_by_name("TypeScriptReact")
                .or_else(|| ps.find_syntax_by_name("TypeScript"))
                .or_else(|| ps.find_syntax_by_extension("tsx"))
                .or_else(|| ps.find_syntax_by_extension("ts")),
            "jsx" => ps
                .find_syntax_by_name("JavaScript (JSX)")
                .or_else(|| ps.find_syntax_by_name("JavaScript"))
                .or_else(|| ps.find_syntax_by_extension("js")),
            "js" => ps
                .find_syntax_by_name("JavaScript")
                .or_else(|| ps.find_syntax_by_extension("js")),
            "rs" => ps
                .find_syntax_by_name("Rust")
                .or_else(|| ps.find_syntax_by_extension("rs")),
            "ex" | "exs" => ps
                .find_syntax_by_name("Elixir")
                .or_else(|| ps.find_syntax_by_extension("ex")),
            "py" => ps
                .find_syntax_by_name("Python")
                .or_else(|| ps.find_syntax_by_extension("py")),
            "sh" => ps
                .find_syntax_by_name("Bourne Again Shell (bash)")
                .or_else(|| ps.find_syntax_by_name("Shell-Unix-Generic"))
                .or_else(|| ps.find_syntax_by_extension("sh")),
            "json" => ps
                .find_syntax_by_name("JSON")
                .or_else(|| ps.find_syntax_by_extension("json")),
            _ => None,
        })
}

fn syntect_to_egui(c: syntect::highlighting::Color) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

fn highlight_job(ui: &egui::Ui, token: &str, body: &str) -> egui::text::LayoutJob {
    let ps = syntax_set();
    let theme = if ui.visuals().dark_mode {
        theme_dark()
    } else {
        theme_light()
    };
    let mono = egui::TextStyle::Monospace.resolve(ui.style());
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = f32::INFINITY;

    let Some(syntax) = find_syntax(ps, token) else {
        job.append(
            body,
            0.0,
            egui::text::TextFormat {
                font_id: mono,
                color: ui.visuals().strong_text_color(),
                ..Default::default()
            },
        );
        return job;
    };

    let mut h = HighlightLines::new(syntax, theme);
    for line in LinesWithEndings::from(body) {
        let Ok(ranges) = h.highlight_line(line, ps) else {
            job.append(
                line,
                0.0,
                egui::text::TextFormat {
                    font_id: mono.clone(),
                    color: ui.visuals().strong_text_color(),
                    ..Default::default()
                },
            );
            continue;
        };
        for (style, text) in ranges {
            let mut color = syntect_to_egui(style.foreground);
            // Lift near-black / near-gray tokens so they stay readable on our panel fill.
            if ui.visuals().dark_mode
                && (color.r() as u16 + color.g() as u16 + color.b() as u16) < 180
            {
                color = egui::Color32::from_rgb(
                    color.r().saturating_add(70),
                    color.g().saturating_add(70),
                    color.b().saturating_add(70),
                );
            }
            job.append(
                text,
                0.0,
                egui::text::TextFormat {
                    font_id: mono.clone(),
                    color,
                    ..Default::default()
                },
            );
        }
    }
    job
}

/// Paint a fenced code block: language chip + highlighted body + copy affordance.
pub fn show_code_fence(ui: &mut egui::Ui, lang: &str, body: &str) {
    let (token, label) = normalize_fence_lang(lang);
    let body = body.trim_matches('\n');
    let fill = ui.visuals().code_bg_color;
    let stroke = ui.visuals().widgets.noninteractive.bg_stroke.color;
    egui::Frame::NONE
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, stroke))
        .corner_radius(crate::theme::RADIUS_MD)
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(&label)
                        .small()
                        .strong()
                        .color(crate::theme::button_colors(ui).accent),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let t = crate::i18n::strings(&crate::prefs::load_preferences().language);
                    if ui.small_button(t.btn_copy).clicked() {
                        ui.ctx().copy_text(body.to_string());
                    }
                });
            });
            ui.add_space(4.0);
            let job = highlight_job(ui, &token, body);
            ui.add(egui::Label::new(job).selectable(true));
        });
    ui.add_space(6.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_maps_model_aliases_to_syntect_tokens() {
        assert_eq!(normalize_fence_lang("javascript").0, "js");
        assert_eq!(normalize_fence_lang("TypeScript").0, "js");
        assert_eq!(normalize_fence_lang("TypeScript").1, "typescript");
        assert_eq!(normalize_fence_lang("react").0, "js");
        assert_eq!(normalize_fence_lang("rust").0, "rs");
        assert_eq!(normalize_fence_lang("elixir").0, "rb");
        assert_eq!(normalize_fence_lang("Elixir").1, "elixir");
    }

    #[test]
    fn syntect_resolves_common_tokens() {
        let ps = syntax_set();
        for token in ["js", "rs", "py", "sh", "json", "rb"] {
            assert!(
                find_syntax(ps, token).is_some(),
                "missing syntect syntax for {token}"
            );
        }
    }
}
