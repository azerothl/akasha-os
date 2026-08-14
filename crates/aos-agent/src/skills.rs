//! Chargement des skills locales (SKILL.md + frontmatter YAML).

use aos_proto::SkillInfo;
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct SkillDoc {
    pub name: String,
    pub description: String,
    pub when_to_use: String,
    pub tools: Vec<String>,
    pub required_caps: Vec<String>,
    pub body: String,
    pub path: PathBuf,
}

#[derive(Debug, Deserialize)]
struct FrontMatter {
    name: Option<String>,
    description: Option<String>,
    #[serde(default)]
    when_to_use: String,
    #[serde(default)]
    tools: Vec<String>,
    #[serde(default)]
    required_caps: Vec<String>,
}

/// Parse un fichier SKILL.md (frontmatter YAML optionnel entre ---).
pub fn parse_skill_md(path: &Path, raw: &str) -> Option<SkillDoc> {
    let (fm, body) = split_frontmatter(raw);
    let meta: FrontMatter = if let Some(yaml) = fm {
        serde_yaml::from_str(yaml).unwrap_or(FrontMatter {
            name: None,
            description: None,
            when_to_use: String::new(),
            tools: vec![],
            required_caps: vec![],
        })
    } else {
        FrontMatter {
            name: None,
            description: None,
            when_to_use: String::new(),
            tools: vec![],
            required_caps: vec![],
        }
    };
    let name = meta
        .name
        .or_else(|| {
            path.parent()
                .and_then(|p| p.file_name())
                .map(|s| s.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| "unnamed".into());
    let description = meta
        .description
        .unwrap_or_else(|| first_line(body).to_string());
    Some(SkillDoc {
        name,
        description,
        when_to_use: meta.when_to_use,
        tools: meta.tools,
        required_caps: meta.required_caps,
        body: body.trim().to_string(),
        path: path.to_path_buf(),
    })
}

fn split_frontmatter(raw: &str) -> (Option<&str>, &str) {
    let raw = raw.trim_start();
    if !raw.starts_with("---") {
        return (None, raw);
    }
    let rest = &raw[3..];
    if let Some(end) = rest.find("\n---") {
        let yaml = rest[..end].trim();
        let body = rest[end + 4..].trim_start();
        return (Some(yaml), body);
    }
    (None, raw)
}

fn first_line(s: &str) -> &str {
    s.lines()
        .map(|l| l.trim().trim_start_matches('#').trim())
        .find(|l| !l.is_empty())
        .unwrap_or("")
}

/// Répertoires de skills : `skills/` (livré) puis `var/skills/` (utilisateur).
pub fn skill_search_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![PathBuf::from("skills"), PathBuf::from("var/skills")];
    if let Ok(home) = std::env::var("AOS_HOME") {
        dirs.insert(0, PathBuf::from(&home).join("var/skills"));
        dirs.insert(0, PathBuf::from(&home).join("skills"));
    }
    dirs
}

fn doc_to_info(doc: SkillDoc) -> SkillInfo {
    SkillInfo {
        name: doc.name,
        description: doc.description,
        when_to_use: doc.when_to_use,
        tools: doc.tools,
        required_caps: doc.required_caps,
        path: doc.path.to_string_lossy().to_string(),
        body: doc.body,
    }
}

pub fn list_skills() -> Vec<SkillInfo> {
    let mut out = Vec::new();
    for dir in skill_search_dirs() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for ent in entries.flatten() {
            let p = ent.path();
            let skill_path = if p.is_dir() {
                let md = p.join("SKILL.md");
                if md.exists() {
                    md
                } else {
                    continue;
                }
            } else if p.extension().and_then(|e| e.to_str()) == Some("md") {
                p.clone()
            } else {
                continue;
            };
            if let Ok(raw) = std::fs::read_to_string(&skill_path) {
                if let Some(doc) = parse_skill_md(&skill_path, &raw) {
                    if !out.iter().any(|s: &SkillInfo| s.name == doc.name) {
                        out.push(doc_to_info(doc));
                    }
                }
            }
        }
    }
    out
}

pub fn load_skills(names: &[String]) -> Vec<SkillDoc> {
    let mut out = Vec::new();
    if names.is_empty() {
        return out;
    }
    for dir in skill_search_dirs() {
        for name in names {
            let candidates = [
                dir.join(name).join("SKILL.md"),
                dir.join(format!("{name}.md")),
            ];
            for c in &candidates {
                if let Ok(raw) = std::fs::read_to_string(c) {
                    if let Some(doc) = parse_skill_md(c, &raw) {
                        if !out.iter().any(|d| d.name == doc.name) {
                            out.push(doc);
                        }
                    }
                }
            }
        }
    }
    out
}

pub fn get_skill(name: &str) -> Option<SkillInfo> {
    list_skills().into_iter().find(|s| s.name == name)
}

/// Fusionne les outils déclarés par les skills dans la sélection.
pub fn merge_skill_tools(selected_tools: &[String], skills: &[SkillDoc]) -> Vec<String> {
    let mut out = selected_tools.to_vec();
    for s in skills {
        for t in &s.tools {
            if !out.contains(t) {
                out.push(t.clone());
            }
        }
    }
    out
}

fn normalize_skill_key(name: &str) -> String {
    name.trim()
        .to_ascii_lowercase()
        .replace(['.', '_'], "-")
}

/// Si `action` est un nom de skill (ou variante `file.author` / `file_author`),
/// renvoie la skill correspondante.
pub fn match_skill_by_action<'a>(action: &str, skills: &'a [SkillDoc]) -> Option<&'a SkillDoc> {
    let key = normalize_skill_key(action);
    skills
        .iter()
        .find(|s| normalize_skill_key(&s.name) == key)
}

/// Message de correction quand le modèle appelle une skill comme outil.
pub fn skill_misuse_hint(action: &str, skill: &SkillDoc) -> String {
    let tools = if skill.tools.is_empty() {
        "(aucun outil déclaré — choisis un outil du catalogue)".to_string()
    } else {
        skill
            .tools
            .iter()
            .map(|t| format!("`{t}`"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!(
        "`{action}` est la skill `{}`, pas un outil. \
         Réessaie avec action = un de : {tools}. \
         Exemple : {{\"thought\":\"…\",\"action\":\"{}\",\"args\":{{…}}}}",
        skill.name,
        skill
            .tools
            .first()
            .map(|s| s.as_str())
            .unwrap_or("web.search")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_frontmatter() {
        let raw = r#"---
name: notes-writer
description: Écrire des notes
tools:
  - notes.create
  - notes.list
---
# Notes writer
Corps de la skill.
"#;
        let doc = parse_skill_md(Path::new("skills/notes-writer/SKILL.md"), raw).unwrap();
        assert_eq!(doc.name, "notes-writer");
        assert_eq!(doc.tools.len(), 2);
        assert!(doc.body.contains("Notes writer"));
    }

    #[test]
    fn match_skill_dot_alias() {
        let skill = SkillDoc {
            name: "file-author".into(),
            description: "files".into(),
            when_to_use: String::new(),
            tools: vec!["fs.write".into(), "files.generate".into()],
            required_caps: vec![],
            body: String::new(),
            path: PathBuf::from("skills/file-author/SKILL.md"),
        };
        let skills = [skill];
        let hit = match_skill_by_action("file.author", &skills).unwrap();
        assert_eq!(hit.name, "file-author");
        let hint = skill_misuse_hint("file.author", hit);
        assert!(hint.contains("fs.write"));
        assert!(hint.contains("skill"));
    }
}
