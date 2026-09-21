//! Illustration project YAML save/load.

use crate::scene::{SceneError, SceneGraph};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Current on-disk project format version.
pub const PROJECT_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectFile {
    pub format_version: u32,
    /// ADR 0011 citation for humans / tooling.
    #[serde(default = "adr_default")]
    pub conventions: String,
    pub scene: SceneGraph,
}

fn adr_default() -> String {
    "ADR-0011".into()
}

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error(transparent)]
    Scene(#[from] SceneError),
    #[error("unsupported format_version {0}")]
    UnsupportedVersion(u32),
    #[error("yaml: {0}")]
    Yaml(String),
}

impl ProjectFile {
    pub fn new(scene: SceneGraph) -> Self {
        Self {
            format_version: PROJECT_FORMAT_VERSION,
            conventions: adr_default(),
            scene,
        }
    }

    pub fn validate(&self) -> Result<(), ProjectError> {
        if self.format_version != PROJECT_FORMAT_VERSION {
            return Err(ProjectError::UnsupportedVersion(self.format_version));
        }
        self.scene.validate()?;
        Ok(())
    }
}

pub fn save_project_yaml(project: &ProjectFile) -> Result<String, ProjectError> {
    project.validate()?;
    serde_yaml::to_string(project).map_err(|e| ProjectError::Yaml(e.to_string()))
}

pub fn load_project_yaml(yaml: &str) -> Result<ProjectFile, ProjectError> {
    let project: ProjectFile =
        serde_yaml::from_str(yaml).map_err(|e| ProjectError::Yaml(e.to_string()))?;
    project.validate()?;
    Ok(project)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::{Quat, Vec3};
    use crate::scene::{NodeKind, SceneNode, Transform};

    #[test]
    fn round_trip_demo_scene() {
        let project = ProjectFile::new(SceneGraph::demo_scene());
        let yaml = save_project_yaml(&project).expect("save");
        assert!(yaml.contains("ADR-0011"));
        assert!(yaml.contains("mesh_box") || yaml.contains("MeshBox") || yaml.contains("box"));
        let loaded = load_project_yaml(&yaml).expect("load");
        assert_eq!(loaded.format_version, PROJECT_FORMAT_VERSION);
        assert_eq!(loaded.scene.roots, project.scene.roots);
        let box_t = &loaded.scene.nodes["box"].transform.translation;
        assert!((box_t.y - 0.5).abs() < 1e-5);
    }

    #[test]
    fn rejects_bad_quat() {
        let mut g = SceneGraph::demo_scene();
        g.nodes.get_mut("box").unwrap().transform.rotation = Quat::from_xyzw(0.0, 0.0, 0.0, 0.0);
        let err = ProjectFile::new(g).validate().unwrap_err();
        assert!(matches!(
            err,
            ProjectError::Scene(SceneError::DegenerateQuaternion)
        ));
    }

    #[test]
    fn identity_rotation_serializes_xyzw() {
        let mut node = SceneNode::empty("n", "N");
        node.kind = NodeKind::Empty;
        node.transform = Transform {
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        };
        let yaml = serde_yaml::to_string(&node).unwrap();
        assert!(yaml.contains("w: 1") || yaml.contains("w: 1.0"));
    }
}
