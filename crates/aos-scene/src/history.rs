//! Bounded project versions and named SceneGraph variants.

use crate::scene::{SceneError, SceneGraph};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum HistoryError {
    #[error("invalid version or variant name")]
    Name,
    #[error("unknown version {0}")]
    Version(u32),
    #[error("unknown variant `{0}`")]
    Variant(String),
    #[error("too many variants")]
    Limit,
    #[error(transparent)]
    Scene(#[from] SceneError),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneVersion {
    pub id: u32,
    pub label: String,
    pub scene: SceneGraph,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneVariant {
    pub name: String,
    pub scene: SceneGraph,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ProjectHistory {
    #[serde(default)]
    pub next_version: u32,
    #[serde(default)]
    pub versions: Vec<SceneVersion>,
    #[serde(default)]
    pub variants: Vec<SceneVariant>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_variant: Option<String>,
}

impl ProjectHistory {
    pub fn validate(&self) -> Result<(), HistoryError> {
        if self.versions.len() > 8 || self.variants.len() > 8 {
            return Err(HistoryError::Limit);
        }
        let mut ids = std::collections::HashSet::new();
        for item in &self.versions {
            validate_name(&item.label)?;
            if !ids.insert(item.id) {
                return Err(HistoryError::Version(item.id));
            }
            item.scene.validate()?;
        }
        let mut names = std::collections::HashSet::new();
        for item in &self.variants {
            validate_name(&item.name)?;
            if !names.insert(item.name.as_str()) {
                return Err(HistoryError::Variant(item.name.clone()));
            }
            item.scene.validate()?;
        }
        Ok(())
    }

    pub fn save_version(&mut self, scene: &SceneGraph, label: &str) -> Result<u32, HistoryError> {
        validate_name(label)?;
        scene.validate()?;
        let id = self.next_version.max(1);
        self.next_version = id.checked_add(1).ok_or(HistoryError::Limit)?;
        if self.versions.len() == 8 {
            self.versions.remove(0);
        }
        self.versions.push(SceneVersion {
            id,
            label: label.trim().into(),
            scene: scene.clone(),
        });
        Ok(id)
    }

    pub fn restore_version(&self, id: u32) -> Result<SceneGraph, HistoryError> {
        self.versions
            .iter()
            .find(|v| v.id == id)
            .map(|v| v.scene.clone())
            .ok_or(HistoryError::Version(id))
    }

    pub fn save_variant(&mut self, scene: &SceneGraph, name: &str) -> Result<(), HistoryError> {
        validate_name(name)?;
        scene.validate()?;
        let name = name.trim();
        if let Some(item) = self.variants.iter_mut().find(|v| v.name == name) {
            item.scene = scene.clone();
        } else if self.variants.len() < 8 {
            self.variants.push(SceneVariant {
                name: name.into(),
                scene: scene.clone(),
            });
        } else {
            return Err(HistoryError::Limit);
        }
        self.active_variant = Some(name.into());
        Ok(())
    }

    pub fn apply_variant(&mut self, name: &str) -> Result<SceneGraph, HistoryError> {
        let scene = self
            .variants
            .iter()
            .find(|v| v.name == name)
            .map(|v| v.scene.clone())
            .ok_or_else(|| HistoryError::Variant(name.into()))?;
        self.active_variant = Some(name.into());
        Ok(scene)
    }
}

fn validate_name(name: &str) -> Result<(), HistoryError> {
    if name.trim().is_empty() || name.len() > 64 {
        Err(HistoryError::Name)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn versions_and_variants_restore_scene() {
        let mut history = ProjectHistory::default();
        let scene = SceneGraph::demo_scene();
        let id = history.save_version(&scene, "first").unwrap();
        history.save_variant(&scene, "ink version").unwrap();
        assert_eq!(history.restore_version(id).unwrap(), scene);
        assert_eq!(history.apply_variant("ink version").unwrap(), scene);
        assert!(history.validate().is_ok());
    }
}
