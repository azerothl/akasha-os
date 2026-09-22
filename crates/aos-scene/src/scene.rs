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
    /// Imported / generated triangle mesh (glTF/GLB). See [`crate::mesh_asset`].
    MeshAsset,
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
    /// Host-resolvable path or pack-relative URI for [`NodeKind::MeshAsset`].
    /// Prefer project-local `/documents/illustrations/**` or pack fixtures.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mesh_uri: Option<String>,
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
            mesh_uri: None,
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
        nodes.insert(humanoid.id.clone(), humanoid);
        nodes.insert(root.id.clone(), root);
        nodes.insert(ground.id.clone(), ground);
        nodes.insert(pedestal.id.clone(), pedestal);
        nodes.insert(box_node.id.clone(), box_node);

        // Articulated HumanoidRig: Empty joints (scale 1) + MeshBox visuals.
        demo_insert_joint_mesh(
            &mut nodes,
            "pelvis",
            "Pelvis",
            "humanoid",
            Vec3::new(0.0, 0.92, 0.0),
            None,
        );
        demo_insert_joint_mesh(
            &mut nodes,
            "spine",
            "Spine",
            "pelvis",
            Vec3::new(0.0, 0.12, 0.0),
            Some((Vec3::ZERO, Vec3::new(0.28, 0.18, 0.16))),
        );
        demo_insert_joint_mesh(
            &mut nodes,
            "chest",
            "Chest",
            "spine",
            Vec3::new(0.0, 0.22, 0.0),
            Some((Vec3::ZERO, Vec3::new(0.4, 0.28, 0.22))),
        );
        demo_insert_joint_mesh(
            &mut nodes,
            "neck",
            "Neck",
            "chest",
            Vec3::new(0.0, 0.2, 0.0),
            None,
        );
        demo_insert_joint_mesh(
            &mut nodes,
            "head",
            "Head",
            "neck",
            Vec3::new(0.0, 0.14, 0.0),
            Some((Vec3::ZERO, Vec3::new(0.22, 0.22, 0.22))),
        );
        for (side, sx) in [("l", -1.0_f32), ("r", 1.0_f32)] {
            let upper = format!("upper_arm_{side}");
            let lower = format!("lower_arm_{side}");
            let hand = format!("hand_{side}");
            demo_insert_joint_mesh(
                &mut nodes,
                &upper,
                &format!("UpperArm{}", side.to_uppercase()),
                "chest",
                Vec3::new(0.28 * sx, 0.08, 0.0),
                Some((Vec3::new(0.0, -0.14, 0.0), Vec3::new(0.1, 0.28, 0.1))),
            );
            demo_insert_joint_mesh(
                &mut nodes,
                &lower,
                &format!("LowerArm{}", side.to_uppercase()),
                &upper,
                Vec3::new(0.0, -0.28, 0.0),
                Some((Vec3::new(0.0, -0.13, 0.0), Vec3::new(0.09, 0.26, 0.09))),
            );
            demo_insert_joint_mesh(
                &mut nodes,
                &hand,
                &format!("Hand{}", side.to_uppercase()),
                &lower,
                Vec3::new(0.0, -0.22, 0.0),
                Some((Vec3::new(0.0, -0.05, 0.0), Vec3::new(0.08, 0.1, 0.06))),
            );
            // Legacy alias for viewport smoke / flat-rig callers.
            let alias = format!("arm_{side}");
            demo_insert_alias(&mut nodes, &alias, &format!("Arm{}", side.to_uppercase()), &upper);

            let uleg = format!("upper_leg_{side}");
            let lleg = format!("lower_leg_{side}");
            let foot = format!("foot_{side}");
            demo_insert_joint_mesh(
                &mut nodes,
                &uleg,
                &format!("UpperLeg{}", side.to_uppercase()),
                "pelvis",
                Vec3::new(0.1 * sx, -0.02, 0.0),
                Some((Vec3::new(0.0, -0.18, 0.0), Vec3::new(0.12, 0.36, 0.12))),
            );
            demo_insert_joint_mesh(
                &mut nodes,
                &lleg,
                &format!("LowerLeg{}", side.to_uppercase()),
                &uleg,
                Vec3::new(0.0, -0.36, 0.0),
                Some((Vec3::new(0.0, -0.17, 0.0), Vec3::new(0.11, 0.34, 0.11))),
            );
            demo_insert_joint_mesh(
                &mut nodes,
                &foot,
                &format!("Foot{}", side.to_uppercase()),
                &lleg,
                Vec3::new(0.0, -0.28, 0.06),
                Some((Vec3::new(0.0, -0.02, 0.04), Vec3::new(0.1, 0.08, 0.2))),
            );
            let lalias = format!("leg_{side}");
            demo_insert_alias(&mut nodes, &lalias, &format!("Leg{}", side.to_uppercase()), &uleg);
        }
        demo_insert_alias(&mut nodes, "torso", "Torso", "chest");

        let mut cam = SceneNode::empty("camera", "Camera");
        cam.kind = NodeKind::Camera;
        cam.parent = Some("root".into());
        // Frame ground + humanoid; identity rotation looks −Z (ADR 0011).
        cam.transform.translation = Vec3::new(0.0, 2.2, 6.5);
        cam.camera = Some(CameraParams::default());
        if let Some(r) = nodes.get_mut("root") {
            if !r.children.contains(&cam.id) {
                r.children.push(cam.id.clone());
            }
        }
        nodes.insert(cam.id.clone(), cam);

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

    /// Set camera optics on a [`NodeKind::Camera`] node (fail-closed on other kinds).
    pub fn set_camera_params(&mut self, id: &str, params: CameraParams) -> Result<(), SceneError> {
        if !params.focal_mm.is_finite()
            || !params.sensor_width_mm.is_finite()
            || !params.near.is_finite()
            || !params.far.is_finite()
            || params.focal_mm <= 0.0
            || params.sensor_width_mm <= 0.0
            || params.near <= 0.0
            || params.far <= params.near
        {
            return Err(SceneError::NonFiniteTransform);
        }
        let node = self
            .nodes
            .get_mut(id)
            .ok_or_else(|| SceneError::UnknownNode(id.into()))?;
        if node.kind != NodeKind::Camera {
            return Err(SceneError::UnknownNode(id.into()));
        }
        node.camera = Some(params);
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

fn demo_attach(nodes: &mut HashMap<String, SceneNode>, parent: &str, child: &str) {
    if let Some(p) = nodes.get_mut(parent) {
        if !p.children.contains(&child.to_string()) {
            p.children.push(child.to_string());
        }
    }
}

fn demo_insert_joint_mesh(
    nodes: &mut HashMap<String, SceneNode>,
    id: &str,
    name: &str,
    parent: &str,
    translation: Vec3,
    mesh: Option<(Vec3, Vec3)>,
) {
    let mut joint = SceneNode::empty(id, name);
    joint.parent = Some(parent.into());
    joint.transform.translation = translation;
    joint.transform.scale = Vec3::ONE;
    demo_attach(nodes, parent, id);
    nodes.insert(joint.id.clone(), joint);
    if let Some((mesh_t, mesh_s)) = mesh {
        let mid = format!("mesh_{id}");
        let mut m = SceneNode::empty(&mid, format!("{name}Mesh"));
        m.kind = NodeKind::MeshBox;
        m.parent = Some(id.into());
        m.transform.translation = mesh_t;
        m.transform.scale = mesh_s;
        demo_attach(nodes, id, &mid);
        nodes.insert(m.id.clone(), m);
    }
}

fn demo_insert_alias(nodes: &mut HashMap<String, SceneNode>, id: &str, name: &str, parent: &str) {
    let mut alias = SceneNode::empty(id, name);
    alias.parent = Some(parent.into());
    alias.visible = false;
    demo_attach(nodes, parent, id);
    nodes.insert(alias.id.clone(), alias);
}
