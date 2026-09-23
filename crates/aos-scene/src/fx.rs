//! Bounded static illustration effects and finishing settings.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectKind {
    Fog,
    Particles,
    Smoke,
    Fire,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneEffect {
    pub kind: EffectKind,
    pub position: [f32; 3],
    pub radius: f32,
    pub intensity: f32,
}

impl SceneEffect {
    pub fn validate(&self) -> bool {
        self.position
            .iter()
            .all(|v| v.is_finite() && v.abs() <= 1_000.0)
            && self.radius.is_finite()
            && (0.05..=100.0).contains(&self.radius)
            && self.intensity.is_finite()
            && (0.0..=1.0).contains(&self.intensity)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderPreset {
    #[serde(default)]
    pub depth_of_field: f32,
    #[serde(default)]
    pub glow: f32,
    #[serde(default)]
    pub color_warmth: f32,
    #[serde(default)]
    pub quality: RenderQuality,
    #[serde(default = "default_width")]
    pub width: u32,
    #[serde(default = "default_height")]
    pub height: u32,
    #[serde(default)]
    pub denoise: bool,
    #[serde(default)]
    pub transparent_background: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RenderQuality {
    Draft,
    #[default]
    Standard,
    Final,
}

impl RenderQuality {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Standard => "standard",
            Self::Final => "final",
        }
    }
}

fn default_width() -> u32 {
    640
}
fn default_height() -> u32 {
    480
}

impl Default for RenderPreset {
    fn default() -> Self {
        Self {
            depth_of_field: 0.0,
            glow: 0.0,
            color_warmth: 0.0,
            quality: RenderQuality::Standard,
            width: default_width(),
            height: default_height(),
            denoise: false,
            transparent_background: false,
        }
    }
}

impl RenderPreset {
    pub fn validate(&self) -> bool {
        [self.depth_of_field, self.glow, self.color_warmth]
            .iter()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
            && (64..=4096).contains(&self.width)
            && (64..=4096).contains(&self.height)
            && u64::from(self.width) * u64::from(self.height) <= 16_777_216
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounds_are_finite() {
        assert!(!SceneEffect {
            kind: EffectKind::Fog,
            position: [f32::NAN, 0.0, 0.0],
            radius: 1.0,
            intensity: 0.5
        }
        .validate());
        assert!(!RenderPreset {
            glow: 2.0,
            ..Default::default()
        }
        .validate());
    }

    #[test]
    fn old_preset_defaults_render_controls() {
        let preset: RenderPreset =
            serde_json::from_str(r#"{"depth_of_field":0.2,"glow":0.3,"color_warmth":0.1}"#)
                .unwrap();
        assert_eq!(preset.quality, RenderQuality::Standard);
        assert_eq!((preset.width, preset.height), (640, 480));
        assert!(!preset.denoise && !preset.transparent_background);
        assert!(preset.validate());
        let too_large = RenderPreset {
            width: 4096,
            height: 4097,
            ..Default::default()
        };
        assert!(!too_large.validate());
    }
}
