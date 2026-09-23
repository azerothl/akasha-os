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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RenderPreset {
    #[serde(default)]
    pub depth_of_field: f32,
    #[serde(default)]
    pub glow: f32,
    #[serde(default)]
    pub color_warmth: f32,
}

impl RenderPreset {
    pub fn validate(&self) -> bool {
        [self.depth_of_field, self.glow, self.color_warmth]
            .iter()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
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
}
