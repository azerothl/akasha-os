//! SceneGraph types (ADR 0011).

use crate::math::{Mat4, Quat, Vec3, EPSILON};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

/// Default project path under the illustrations document tree.
pub const DEFAULT_SCENE_YAML_PATH: &str = "/documents/illustrations/project.scene.yaml";

/// Document prefix for Illustration Studio projects.
pub const ILLUSTRATIONS_DOCUMENTS_PREFIX: &str = "/documents/illustrations/";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    #[default]
    Empty,
    MeshBox,
    Camera,
    Light,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    pub translation: Vec3,
    /// Unit quaternion `[x,y,z,w]`.
    pub rotation: Quat,
    pub scale: Vec3,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        }
    }
}

impl Transform {
    pub fn to_matrix(&self) -> Mat4 {
        Mat4::from_trs(self.translation, self.rotation, self.scale)
    }

    pub fn validate(&self) -> Result<(), SceneError> {
        if !self.translation.is_finite() || !self.scale.is_finite() || !self.rotation.is_finite() {
            return Err(SceneError::NonFiniteTransform);
        }
        let len = self.rotation.length();
        if len < EPSILON {
            return Err(SceneError::DegenerateQuaternion);
        }
        Ok(())
    }

    pub fn normalized_rotation(mut self) -> Result<Self, SceneError> {
        self.validate()?;
        self.rotation = self
            .rotation
            .normalized()
            .ok_or(SceneError::DegenerateQuaternion)?;
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CameraParams {
    /// Focal length in millimetres.
    pub focal_mm: f32,
    /// Horizontal sensor width in millimetres.
    pub sensor_width_mm: f32,
    /// Near clip in metres (> 0).
    pub near: f32,
    /// Far clip in metres.
    pub far: f32,
}

impl Default for CameraParams {
    fn default() -> Self {
        Self {
            focal_mm: 50.0,
            sensor_width_mm: 36.0,
            near: 0.1,
            far: 100.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneNode {
    pub id: String,
    pub name: String,
    pub kind: NodeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(default)]
    pub children: Vec<String>,
    #[serde(default)]
    pub transform: Transform,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub camera: Option<CameraParams>,
    #[serde(default)]
    pub visible: bool,
}

impl SceneNode {
    pub fn empty(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            kind: NodeKind::Empty,
            parent: None,
            children: Vec::new(),
            transform: Transform::default(),
            camera: None,
            visible: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneGraph {
    #[serde(default)]
    pub nodes: HashMap<String, SceneNode>,
    /// Root node ids (no parent).
    #[serde(default)]
    pub roots: Vec<String>,
    /// Active camera node id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_camera: Option<String>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SceneError {
    #[error("non-finite transform")]
    NonFiniteTransform,
    #[error("degenerate quaternion")]
    DegenerateQuaternion,
    #[error("unknown node `{0}`")]
    UnknownNode(String),
    #[error("duplicate node `{0}`")]
    DuplicateNode(String),
    #[error("cycle detected at `{0}`")]
    Cycle(String),
}

impl Default for SceneGraph {
    fn default() -> Self {
        Self::demo_scene()
    }
}

impl SceneGraph {
    /// Preview starter: ground, pedestal, prop box, placeholder humanoid, camera.
    /// Keeps `box` / `camera` ids used by ADR golden tests.
    pub fn demo_scene() -> Self {
        let mut nodes = HashMap::new();

        let mut root = SceneNode::empty("root", "Scene");
        root.children = vec![
            "ground".into(),
            "pedestal".into(),
            "box".into(),
            "humanoid".into(),
            "camera".into(),
        ];

        let mut ground = SceneNode::empty("ground", "Ground");
        ground.kind = NodeKind::MeshBox;
        ground.parent = Some("root".into());
        ground.transform.translation = Vec3::new(0.0, 0.04, 0.0);
        ground.transform.scale = Vec3::new(6.0, 0.08, 6.0);

        let mut pedestal = SceneNode::empty("pedestal", "Pedestal");
        pedestal.kind = NodeKind::MeshBox;
        pedestal.parent = Some("root".into());
        pedestal.transform.translation = Vec3::new(0.0, 0.175, 0.0);
        pedestal.transform.scale = Vec3::new(1.2, 0.35, 1.2);

        // Box sits on the pedestal (centre height ≈ pedestal top + half box).
        let mut box_node = SceneNode::empty("box", "Box");
        box_node.kind = NodeKind::MeshBox;
        box_node.parent = Some("root".into());
        box_node.transform.translation = Vec3::new(0.0, 0.675, 0.0);
        box_node.transform.scale = Vec3::new(0.7, 0.7, 0.7);

        let mut humanoid = SceneNode::empty("humanoid", "Humanoid");
        humanoid.parent = Some("root".into());
        humanoid.transform.translation = Vec3::new(1.4, 0.0, 0.3);
        humanoid.children = vec![
            "torso".into(),
            "head".into(),
            "leg_l".into(),
            "leg_r".into(),
            "arm_l".into(),
            "arm_r".into(),
        ];

        let mut torso = SceneNode::empty("torso", "Torso");
        torso.kind = NodeKind::MeshBox;
        torso.parent = Some("humanoid".into());
        torso.transform.translation = Vec3::new(0.0, 1.0, 0.0);
        torso.transform.scale = Vec3::new(0.45, 0.55, 0.25);

        let mut head = SceneNode::empty("head", "Head");
        head.kind = NodeKind::MeshBox;
        head.parent = Some("humanoid".into());
        head.transform.translation = Vec3::new(0.0, 1.55, 0.0);
        head.transform.scale = Vec3::new(0.22, 0.22, 0.22);

        let mut leg_l = SceneNode::empty("leg_l", "LegL");
        leg_l.kind = NodeKind::MeshBox;
        leg_l.parent = Some("humanoid".into());
        leg_l.transform.translation = Vec3::new(-0.12, 0.35, 0.0);
        leg_l.transform.scale = Vec3::new(0.14, 0.7, 0.14);

        let mut leg_r = SceneNode::empty("leg_r", "LegR");
        leg_r.kind = NodeKind::MeshBox;
        leg_r.parent = Some("humanoid".into());
        leg_r.transform.translation = Vec3::new(0.12, 0.35, 0.0);
        leg_r.transform.scale = Vec3::new(0.14, 0.7, 0.14);

        let mut arm_l = SceneNode::empty("arm_l", "ArmL");
        arm_l.kind = NodeKind::MeshBox;
        arm_l.parent = Some("humanoid".into());
        arm_l.transform.translation = Vec3::new(-0.38, 1.05, 0.0);
        arm_l.transform.scale = Vec3::new(0.12, 0.5, 0.12);

        let mut arm_r = SceneNode::empty("arm_r", "ArmR");
        arm_r.kind = NodeKind::MeshBox;
        arm_r.parent = Some("humanoid".into());
        arm_r.transform.translation = Vec3::new(0.38, 1.05, 0.0);
        arm_r.transform.scale = Vec3::new(0.12, 0.5, 0.12);

        let mut cam = SceneNode::empty("camera", "Camera");
        cam.kind = NodeKind::Camera;
        cam.parent = Some("root".into());
        // Frame ground + humanoid; identity rotation looks −Z (ADR 0011).
        cam.transform.translation = Vec3::new(0.0, 2.2, 6.5);
        cam.camera = Some(CameraParams::default());

        for n in [
            root, ground, pedestal, box_node, humanoid, torso, head, leg_l, leg_r, arm_l, arm_r,
            cam,
        ] {
            nodes.insert(n.id.clone(), n);
        }

        Self {
            nodes,
            roots: vec!["root".into()],
            active_camera: Some("camera".into()),
        }
    }

    pub fn validate(&self) -> Result<(), SceneError> {
        for (id, node) in &self.nodes {
            if id != &node.id {
                return Err(SceneError::UnknownNode(format!(
                    "map key `{id}` != node.id `{}`",
                    node.id
                )));
            }
            node.transform.validate()?;
            if let Some(p) = &node.parent {
                if !self.nodes.contains_key(p) {
                    return Err(SceneError::UnknownNode(p.clone()));
                }
            }
            for c in &node.children {
                if !self.nodes.contains_key(c) {
                    return Err(SceneError::UnknownNode(c.clone()));
                }
            }
        }
        for root in &self.roots {
            self.world_matrix(root)?;
        }
        Ok(())
    }

    pub fn local_matrix(&self, id: &str) -> Result<Mat4, SceneError> {
        let node = self
            .nodes
            .get(id)
            .ok_or_else(|| SceneError::UnknownNode(id.into()))?;
        let t = node.transform.clone().normalized_rotation()?;
        Ok(t.to_matrix())
    }

    /// World = parent_world * local (ADR 0011 parenting).
    pub fn world_matrix(&self, id: &str) -> Result<Mat4, SceneError> {
        let mut chain = Vec::new();
        let mut cur = Some(id.to_string());
        let mut guard = 0usize;
        while let Some(cid) = cur {
            if guard > self.nodes.len() + 2 {
                return Err(SceneError::Cycle(cid));
            }
            guard += 1;
            let node = self
                .nodes
                .get(&cid)
                .ok_or_else(|| SceneError::UnknownNode(cid.clone()))?;
            chain.push(cid.clone());
            cur = node.parent.clone();
        }
        chain.reverse();
        let mut world = Mat4::IDENTITY;
        for cid in chain {
            world = world * self.local_matrix(&cid)?;
        }
        Ok(world)
    }

    pub fn world_translation(&self, id: &str) -> Result<Vec3, SceneError> {
        Ok(self.world_matrix(id)?.transform_point(Vec3::ZERO))
    }

    /// Identity camera looks along **−Z** in local space.
    pub fn camera_forward_world(&self, id: &str) -> Result<Vec3, SceneError> {
        let m = self.world_matrix(id)?;
        Ok(m.transform_vector(Vec3::new(0.0, 0.0, -1.0))
            .normalized()
            .unwrap_or(Vec3::new(0.0, 0.0, -1.0)))
    }

    pub fn set_transform(&mut self, id: &str, transform: Transform) -> Result<(), SceneError> {
        let t = transform.normalized_rotation()?;
        let node = self
            .nodes
            .get_mut(id)
            .ok_or_else(|| SceneError::UnknownNode(id.into()))?;
        node.transform = t;
        Ok(())
    }

    pub fn node_ids_depth_first(&self) -> Vec<String> {
        let mut out = Vec::new();
        fn walk(g: &SceneGraph, id: &str, out: &mut Vec<String>) {
            out.push(id.to_string());
            if let Some(n) = g.nodes.get(id) {
                for c in &n.children {
                    walk(g, c, out);
                }
            }
        }
        for r in &self.roots {
            walk(self, r, &mut out);
        }
        out
    }
}
