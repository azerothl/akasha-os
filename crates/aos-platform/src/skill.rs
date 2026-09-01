//! Skills déclaratives (F-EXT) — recettes markdown sans nouveau binaire.
//!
//! Stockage : `var/skills/<name>/{skill.yaml,SKILL.md}`.

use aos_proto::{SkillCreateRequest, SkillInfo};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const MAX_BODY_BYTES: usize = 64 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum SkillError {
    #[error("nom invalide (attendu [a-z][a-z0-9-]{{1,32}}): {0}")]
    BadName(String),
    #[error("corps trop volumineux (max {MAX_BODY_BYTES} octets)")]
    BodyTooLarge,
    #[error("skill inconnue: {0}")]
    NotFound(String),
    #[error("déjà existante: {0}")]
    Exists(String),
    #[error("io: {0}")]
    Io(String),
    #[error("revue de caps requise: {0}")]
    CapReviewRequired(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SkillYaml {
    name: String,
    description: String,
    #[serde(default)]
    when_to_use: String,
    #[serde(default)]
    tools: Vec<String>,
    #[serde(default)]
    required_caps: Vec<String>,
}

/// Magasin de skills utilisateur.
pub struct SkillStore {
    dir: PathBuf,
}

impl SkillStore {
    pub fn open(dir: impl AsRef<Path>) -> Result<Self, SkillError> {
        let dir = dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&dir).map_err(|e| SkillError::Io(e.to_string()))?;
        Ok(Self { dir })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn validate_name(name: &str) -> Result<(), SkillError> {
        let ok = name.len() >= 2
            && name.len() <= 33
            && name
                .chars()
                .enumerate()
                .all(|(i, c)| {
                    if i == 0 {
                        c.is_ascii_lowercase()
                    } else {
                        c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'
                    }
                });
        if ok {
            Ok(())
        } else {
            Err(SkillError::BadName(name.into()))
        }
    }

    pub fn create(&self, req: &SkillCreateRequest) -> Result<SkillInfo, SkillError> {
        Self::validate_name(&req.name)?;
        if req.body.len() > MAX_BODY_BYTES {
            return Err(SkillError::BodyTooLarge);
        }
        let dest = self.dir.join(&req.name);
        if dest.exists() {
            return Err(SkillError::Exists(req.name.clone()));
        }
        std::fs::create_dir_all(&dest).map_err(|e| SkillError::Io(e.to_string()))?;
        let yaml = SkillYaml {
            name: req.name.clone(),
            description: req.description.clone(),
            when_to_use: req.when_to_use.clone(),
            tools: req.tools.clone(),
            required_caps: req.required_caps.clone(),
        };
        let yaml_s =
            serde_yaml::to_string(&yaml).map_err(|e| SkillError::Io(e.to_string()))?;
        std::fs::write(dest.join("skill.yaml"), yaml_s)
            .map_err(|e| SkillError::Io(e.to_string()))?;
        let md = format!(
            "---\nname: {}\ndescription: {}\nwhen_to_use: {}\ntools:\n{}required_caps:\n{}---\n\n{}\n",
            req.name,
            escape_yaml_scalar(&req.description),
            escape_yaml_scalar(&req.when_to_use),
            req.tools
                .iter()
                .map(|t| format!("  - {t}\n"))
                .collect::<String>(),
            req.required_caps
                .iter()
                .map(|c| format!("  - {c}\n"))
                .collect::<String>(),
            req.body.trim()
        );
        std::fs::write(dest.join("SKILL.md"), md).map_err(|e| SkillError::Io(e.to_string()))?;
        self.describe(&req.name)
    }

    pub fn list(&self) -> Vec<SkillInfo> {
        let mut out = Vec::new();
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return out;
        };
        for ent in entries.flatten() {
            let p = ent.path();
            if !p.is_dir() {
                continue;
            }
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if let Ok(info) = self.describe(name) {
                out.push(info);
            }
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    pub fn describe(&self, name: &str) -> Result<SkillInfo, SkillError> {
        let dest = self.dir.join(name);
        if !dest.exists() {
            return Err(SkillError::NotFound(name.into()));
        }
        let yaml_path = dest.join("skill.yaml");
        let md_path = dest.join("SKILL.md");
        let body = std::fs::read_to_string(&md_path).unwrap_or_default();
        let body_only = strip_frontmatter(&body);
        if yaml_path.exists() {
            let raw = std::fs::read_to_string(&yaml_path).map_err(|e| SkillError::Io(e.to_string()))?;
            let y: SkillYaml =
                serde_yaml::from_str(&raw).map_err(|e| SkillError::Io(e.to_string()))?;
            return Ok(SkillInfo {
                name: y.name,
                description: y.description,
                when_to_use: y.when_to_use,
                tools: y.tools,
                required_caps: y.required_caps,
                path: md_path.to_string_lossy().to_string(),
                body: body_only,
            });
        }
        // Fallback : parse SKILL.md only
        Ok(SkillInfo {
            name: name.into(),
            description: first_line(&body_only).to_string(),
            when_to_use: String::new(),
            tools: vec![],
            required_caps: vec![],
            path: md_path.to_string_lossy().to_string(),
            body: body_only,
        })
    }

    /// Install a SKILL.md from a signed catalogue (hash already checked by caller).
    /// `approved_caps` is required when the recipe lists `required_caps`.
    pub fn install_from_markdown(
        &self,
        name: &str,
        md_bytes: &[u8],
        approved_caps: Option<Vec<String>>,
    ) -> Result<SkillInfo, SkillError> {
        Self::validate_name(name)?;
        if md_bytes.len() > MAX_BODY_BYTES {
            return Err(SkillError::BodyTooLarge);
        }
        let raw = std::str::from_utf8(md_bytes)
            .map_err(|e| SkillError::Io(e.to_string()))?;
        let required = required_caps_from_skill_md(raw);
        let granted = match approved_caps {
            Some(caps) => caps,
            None if required.is_empty() => Vec::new(),
            None => return Err(SkillError::CapReviewRequired(required.join(", "))),
        };
        for cap in &granted {
            if !required.contains(cap) {
                return Err(SkillError::Io(format!(
                    "cap approuvée non demandée par le skill: {cap}"
                )));
            }
        }
        let dest = self.dir.join(name);
        if dest.exists() {
            return Err(SkillError::Exists(name.into()));
        }
        std::fs::create_dir_all(&dest).map_err(|e| SkillError::Io(e.to_string()))?;
        std::fs::write(dest.join("SKILL.md"), md_bytes)
            .map_err(|e| SkillError::Io(e.to_string()))?;
        self.describe(name)
    }

    pub fn uninstall(&self, name: &str) -> Result<(), SkillError> {
        let dest = self.dir.join(name);
        if !dest.exists() {
            return Err(SkillError::NotFound(name.into()));
        }
        std::fs::remove_dir_all(&dest).map_err(|e| SkillError::Io(e.to_string()))?;
        Ok(())
    }
}

fn escape_yaml_scalar(s: &str) -> String {
    if s.contains(':') || s.contains('#') || s.contains('\n') || s.contains('"') {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        s.to_string()
    }
}

fn strip_frontmatter(raw: &str) -> String {
    let raw = raw.trim_start();
    if !raw.starts_with("---") {
        return raw.to_string();
    }
    let rest = &raw[3..];
    if let Some(end) = rest.find("\n---") {
        return rest[end + 4..].trim().to_string();
    }
    raw.to_string()
}

fn required_caps_from_skill_md(raw: &str) -> Vec<String> {
    let raw = raw.trim_start();
    if !raw.starts_with("---") {
        return Vec::new();
    }
    let rest = &raw[3..];
    let Some(end) = rest.find("\n---") else {
        return Vec::new();
    };
    let yaml = rest[..end].trim();
    #[derive(Deserialize)]
    struct CapsOnly {
        #[serde(default)]
        required_caps: Vec<String>,
    }
    serde_yaml::from_str::<CapsOnly>(yaml)
        .map(|c| c.required_caps)
        .unwrap_or_default()
}

fn first_line(s: &str) -> &str {
    s.lines()
        .map(|l| l.trim().trim_start_matches('#').trim())
        .find(|l| !l.is_empty())
        .unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_list_uninstall() {
        let dir = std::env::temp_dir().join(format!("aos-skill-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = SkillStore::open(&dir).unwrap();
        let info = store
            .create(&SkillCreateRequest {
                name: "demo-skill".into(),
                description: "demo".into(),
                when_to_use: "tests".into(),
                tools: vec!["notes.list".into()],
                required_caps: vec![],
                body: "Utilise notes.list.".into(),
                actor: "agent:1".into(),
                actor_caps: vec![],
            })
            .unwrap();
        assert_eq!(info.name, "demo-skill");
        assert_eq!(store.list().len(), 1);
        store.uninstall("demo-skill").unwrap();
        assert!(store.list().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_from_markdown_requires_cap_review() {
        let dir = std::env::temp_dir().join(format!("aos-skill-caps-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = SkillStore::open(&dir).unwrap();
        let md = b"---\nname: gated\ndescription: x\nrequired_caps:\n  - fs.read:/documents/**\n---\n\nDo the thing.\n";
        let err = store
            .install_from_markdown("gated", md, None)
            .unwrap_err();
        assert!(matches!(err, SkillError::CapReviewRequired(_)));
        store
            .install_from_markdown(
                "gated",
                md,
                Some(vec!["fs.read:/documents/**".into()]),
            )
            .unwrap();
        assert_eq!(store.list().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_from_markdown_empty_caps_no_review() {
        let dir = std::env::temp_dir().join(format!("aos-skill-nocaps-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = SkillStore::open(&dir).unwrap();
        let md = include_bytes!("../../../community/skills/morning-brief/SKILL.md");
        store
            .install_from_markdown("morning-brief", md, None)
            .unwrap();
        assert_eq!(store.describe("morning-brief").unwrap().name, "morning-brief");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
