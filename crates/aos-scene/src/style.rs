//! NPR style packs — data-driven Sketch / Pencil / Ink (Illustration Studio).
//!
//! Styles are YAML under `/assets/illustration/styles/**` and embedded for
//! offline / CI. They describe appearance only; SceneGraph remains SoT.
//! Render presets (resolution, samples) stay separate (spec §41).

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Logical prefix for style pack trees (same asset root as mesh packs).
pub const ILLUSTRATION_STYLES_PREFIX: &str = "/assets/illustration/styles/";

/// On-disk style pack format version.
pub const STYLE_PACK_FORMAT_VERSION: u32 = 1;

/// Embedded traditional-drawing pack (offline defaults).
pub const EMBEDDED_STYLE_PACK_MANIFEST_YAML: &str = include_str!(
    "../../../share/assets/illustration/styles/traditional-drawing/manifest.yaml"
);
pub const EMBEDDED_STYLE_SKETCH_YAML: &str =
    include_str!("../../../share/assets/illustration/styles/traditional-drawing/sketch.yaml");
pub const EMBEDDED_STYLE_PENCIL_YAML: &str =
    include_str!("../../../share/assets/illustration/styles/traditional-drawing/pencil.yaml");
pub const EMBEDDED_STYLE_INK_YAML: &str =
    include_str!("../../../share/assets/illustration/styles/traditional-drawing/ink.yaml");

/// Default DeclUI / beauty style when the user has not chosen one yet.
pub const DEFAULT_STYLE_ID: &str = "pencil";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StyleFamily {
    Sketch,
    Pencil,
    Ink,
}

impl StyleFamily {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sketch => "sketch",
            Self::Pencil => "pencil",
            Self::Ink => "ink",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "sketch" | "sketch_loose" | "esquisse" => Some(Self::Sketch),
            "pencil" | "pencil_classic" | "crayon" => Some(Self::Pencil),
            "ink" | "ink_clean" | "encre" => Some(Self::Ink),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StylePackManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub format_version: u32,
    #[serde(default = "adr_default")]
    pub conventions: String,
    pub styles: Vec<String>,
}

fn adr_default() -> String {
    "ADR-0011".into()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StyleDef {
    pub id: String,
    pub family: StyleFamily,
    #[serde(default = "npr_renderer")]
    pub renderer: String,
    #[serde(default)]
    pub line: StyleLine,
    #[serde(default)]
    pub shading: StyleShading,
    #[serde(default)]
    pub color: StyleColor,
    #[serde(default)]
    pub paper: StylePaper,
    #[serde(default)]
    pub aliases: Vec<String>,
}

fn npr_renderer() -> String {
    "npr".into()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StyleLine {
    #[serde(default = "default_line_type")]
    pub r#type: String,
    #[serde(default = "default_line_width")]
    pub width: f32,
    #[serde(default)]
    pub jitter: f32,
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    #[serde(default)]
    pub width_min: Option<f32>,
    #[serde(default)]
    pub width_max: Option<f32>,
}

fn default_line_type() -> String {
    "pencil".into()
}
fn default_line_width() -> f32 {
    1.0
}
fn default_opacity() -> f32 {
    1.0
}

impl Default for StyleLine {
    fn default() -> Self {
        Self {
            r#type: default_line_type(),
            width: default_line_width(),
            jitter: 0.0,
            opacity: default_opacity(),
            width_min: None,
            width_max: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StyleShading {
    #[serde(default = "default_shading_type")]
    pub r#type: String,
    #[serde(default = "default_contrast")]
    pub contrast: f32,
}

fn default_shading_type() -> String {
    "none".into()
}
fn default_contrast() -> f32 {
    0.5
}

impl Default for StyleShading {
    fn default() -> Self {
        Self {
            r#type: default_shading_type(),
            contrast: default_contrast(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StyleColor {
    #[serde(default)]
    pub saturation: f32,
}

impl Default for StyleColor {
    fn default() -> Self {
        Self { saturation: 0.0 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StylePaper {
    #[serde(default = "default_paper_texture")]
    pub texture: String,
    /// RGB paper tint 0..=255.
    #[serde(default = "default_paper_tint")]
    pub tint: [u8; 3],
}

fn default_paper_texture() -> String {
    "plain".into()
}
fn default_paper_tint() -> [u8; 3] {
    [248, 246, 240]
}

impl Default for StylePaper {
    fn default() -> Self {
        Self {
            texture: default_paper_texture(),
            tint: default_paper_tint(),
        }
    }
}

/// Compact runtime view used by CPU / Blender mock / export.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ResolvedStyle {
    pub id: String,
    pub family: StyleFamily,
    pub line_width: f32,
    pub jitter: f32,
    pub opacity: f32,
    pub shading: String,
    pub contrast: f32,
    pub paper_tint: [u8; 3],
    pub paper_texture: String,
}

impl ResolvedStyle {
    pub fn from_def(def: &StyleDef) -> Self {
        Self {
            id: def.id.clone(),
            family: def.family,
            line_width: def.line.width.max(0.1),
            jitter: def.line.jitter.clamp(0.0, 1.0),
            opacity: def.line.opacity.clamp(0.0, 1.0),
            shading: def.shading.r#type.clone(),
            contrast: def.shading.contrast.clamp(0.0, 1.0),
            paper_tint: def.paper.tint,
            paper_texture: def.paper.texture.clone(),
        }
    }

    /// Ink / graphite stroke colour for CPU approximation.
    pub fn stroke_rgb(&self) -> [u8; 3] {
        match self.family {
            StyleFamily::Sketch => [90, 82, 70],
            StyleFamily::Pencil => [48, 46, 44],
            StyleFamily::Ink => [12, 12, 14],
        }
    }

    /// Soft fill colour under hatch (when shading is enabled).
    pub fn fill_rgb(&self) -> [u8; 3] {
        match self.family {
            StyleFamily::Sketch => [230, 226, 214],
            StyleFamily::Pencil => [210, 206, 196],
            StyleFamily::Ink => [236, 236, 232],
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum StyleError {
    #[error("unsupported style pack format_version {0}")]
    UnsupportedVersion(u32),
    #[error("unknown style `{0}`")]
    UnknownStyle(String),
    #[error("yaml: {0}")]
    Yaml(String),
}

impl StylePackManifest {
    pub fn validate(&self) -> Result<(), StyleError> {
        if self.format_version != STYLE_PACK_FORMAT_VERSION {
            return Err(StyleError::UnsupportedVersion(self.format_version));
        }
        Ok(())
    }
}

/// Parse a single style document.
pub fn load_style_yaml(yaml: &str) -> Result<StyleDef, StyleError> {
    serde_yaml::from_str(yaml).map_err(|e| StyleError::Yaml(e.to_string()))
}

/// Parse a style pack manifest.
pub fn load_style_pack_manifest_yaml(yaml: &str) -> Result<StylePackManifest, StyleError> {
    let m: StylePackManifest =
        serde_yaml::from_str(yaml).map_err(|e| StyleError::Yaml(e.to_string()))?;
    m.validate()?;
    Ok(m)
}

/// Embedded traditional-drawing styles (Sketch / Pencil / Ink).
pub fn embedded_styles() -> Vec<StyleDef> {
    [
        EMBEDDED_STYLE_SKETCH_YAML,
        EMBEDDED_STYLE_PENCIL_YAML,
        EMBEDDED_STYLE_INK_YAML,
    ]
    .into_iter()
    .map(|y| load_style_yaml(y).expect("embedded style yaml"))
    .collect()
}

/// Resolve a style id or alias against the embedded pack (fail-closed).
pub fn resolve_style(id: &str) -> Result<ResolvedStyle, StyleError> {
    let key = id.trim();
    if key.is_empty() {
        return Err(StyleError::UnknownStyle(id.into()));
    }
    let lower = key.to_ascii_lowercase();
    let styles = embedded_styles();
    for def in &styles {
        if def.id.eq_ignore_ascii_case(&lower) {
            return Ok(ResolvedStyle::from_def(def));
        }
        if def
            .aliases
            .iter()
            .any(|a| a.eq_ignore_ascii_case(&lower))
        {
            return Ok(ResolvedStyle::from_def(def));
        }
    }
    if let Some(family) = StyleFamily::parse(&lower) {
        if let Some(def) = styles.iter().find(|d| d.family == family) {
            return Ok(ResolvedStyle::from_def(def));
        }
    }
    Err(StyleError::UnknownStyle(id.into()))
}

/// Optional style: empty / absent → `None`; unknown → error (fail-closed).
pub fn parse_optional_style(s: Option<&str>) -> Result<Option<ResolvedStyle>, StyleError> {
    match s {
        None | Some("") => Ok(None),
        Some(v) => resolve_style(v).map(Some),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_pack_loads() {
        let m = load_style_pack_manifest_yaml(EMBEDDED_STYLE_PACK_MANIFEST_YAML).unwrap();
        assert_eq!(m.id, "traditional-drawing");
        assert_eq!(m.styles, vec!["sketch", "pencil", "ink"]);
        assert_eq!(embedded_styles().len(), 3);
    }

    #[test]
    fn resolve_aliases() {
        assert_eq!(resolve_style("pencil_classic").unwrap().family, StyleFamily::Pencil);
        assert_eq!(resolve_style("esquisse").unwrap().family, StyleFamily::Sketch);
        assert_eq!(resolve_style("encre").unwrap().family, StyleFamily::Ink);
        assert_eq!(resolve_style("INK").unwrap().id, "ink");
    }

    #[test]
    fn unknown_style_fails_closed() {
        assert!(matches!(
            resolve_style("watercolor_wet"),
            Err(StyleError::UnknownStyle(_))
        ));
    }

    #[test]
    fn optional_empty_is_none() {
        assert!(parse_optional_style(None).unwrap().is_none());
        assert!(parse_optional_style(Some("")).unwrap().is_none());
    }
}
