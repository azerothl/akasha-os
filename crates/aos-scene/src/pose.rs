//! Humanoid pose / IK-lite (FK joints + look-at + arm raise).
//!
//! Operates on SceneGraph transforms only (ADR 0011). Not a full IK solver.

use crate::math::{Quat, Vec3};
use crate::ops::{SceneOp, UndoStack};
use crate::scene::{SceneError, SceneGraph, Transform};
use thiserror::Error;

/// DeclUI / host service id: apply a pose op to a humanoid subtree.
pub const SCENE_POSE_SERVICE: &str = "scene.pose";

/// Cap: apply pose / IK-lite ops (fail-closed).
pub const SCENE_POSE_CAP: &str = "scene.pose";

#[derive(Debug, Error, PartialEq)]
pub enum PoseError {
    #[error("scene: {0}")]
    Scene(String),
    #[error("unknown joint `{0}` under `{1}`")]
    UnknownJoint(String, String),
    #[error("unknown pose preset `{0}`")]
    UnknownPreset(String),
    #[error("missing humanoid root `{0}`")]
    MissingRoot(String),
}

impl From<SceneError> for PoseError {
    fn from(e: SceneError) -> Self {
        Self::Scene(e.to_string())
    }
}

/// Named joints relative to a humanoid prefab root (suffix after instance prefix).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JointId {
    Root,
    Torso,
    Head,
    ArmL,
    ArmR,
    LegL,
    LegR,
}

impl JointId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Root => "",
            Self::Torso => "torso",
            Self::Head => "head",
            Self::ArmL => "arm_l",
            Self::ArmR => "arm_r",
            Self::LegL => "leg_l",
            Self::LegR => "leg_r",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "" | "root" | "humanoid" => Some(Self::Root),
            "torso" => Some(Self::Torso),
            "head" => Some(Self::Head),
            "arm_l" | "arml" | "left_arm" => Some(Self::ArmL),
            "arm_r" | "armr" | "right_arm" => Some(Self::ArmR),
            "leg_l" | "legl" | "left_leg" => Some(Self::LegL),
            "leg_r" | "legr" | "right_leg" => Some(Self::LegR),
            _ => None,
        }
    }
}

/// Pose operation (applied as undoable `SetTransform` batch).
#[derive(Debug, Clone, PartialEq)]
pub enum PoseOp {
    /// Rotate a joint by axis-angle (radians) in local space (FK).
    RotateJoint {
        humanoid_root: String,
        joint: JointId,
        axis: Vec3,
        angle_rad: f32,
    },
    /// Yaw/pitch the head toward a world-space point (IK-lite look-at).
    LookAt {
        humanoid_root: String,
        target_world: Vec3,
    },
    /// Named preset applied to a humanoid root.
    Preset {
        humanoid_root: String,
        preset: PosePreset,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PosePreset {
    Rest,
    WaveRight,
    WaveLeft,
    ReachForward,
    LookLeft,
    LookRight,
}

impl PosePreset {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "rest" | "default" | "t_pose" | "tpose" => Some(Self::Rest),
            "wave_right" | "waveright" | "wave" => Some(Self::WaveRight),
            "wave_left" | "waveleft" => Some(Self::WaveLeft),
            "reach_forward" | "reach" | "point" => Some(Self::ReachForward),
            "look_left" | "lookleft" => Some(Self::LookLeft),
            "look_right" | "lookright" => Some(Self::LookRight),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rest => "rest",
            Self::WaveRight => "wave_right",
            Self::WaveLeft => "wave_left",
            Self::ReachForward => "reach_forward",
            Self::LookLeft => "look_left",
            Self::LookRight => "look_right",
        }
    }
}

/// Resolve node id for a joint under a humanoid instance root.
pub fn joint_node_id(scene: &SceneGraph, humanoid_root: &str, joint: JointId) -> Result<String, PoseError> {
    if joint == JointId::Root {
        if scene.nodes.contains_key(humanoid_root) {
            return Ok(humanoid_root.to_string());
        }
        return Err(PoseError::MissingRoot(humanoid_root.into()));
    }
    let suffix = joint.as_str();
    // Prefab instantiate uses `{prefix}{local_id}` — accept exact child name match too.
    if let Some(node) = scene.nodes.get(humanoid_root) {
        for child in &node.children {
            if child == suffix
                || child.ends_with(&format!("_{suffix}"))
                || child.ends_with(suffix)
            {
                // Prefer exact suffix / name match among children.
                if let Some(c) = scene.nodes.get(child) {
                    let name_l = c.name.to_ascii_lowercase();
                    let want = match joint {
                        JointId::Torso => "torso",
                        JointId::Head => "head",
                        JointId::ArmL => "arml",
                        JointId::ArmR => "armr",
                        JointId::LegL => "legl",
                        JointId::LegR => "legr",
                        JointId::Root => "",
                    };
                    let id_l = child.to_ascii_lowercase();
                    if id_l.ends_with(suffix)
                        || name_l.replace(' ', "") == want
                        || name_l == suffix
                    {
                        return Ok(child.clone());
                    }
                }
            }
        }
        // Fallback: scan descendants by id suffix.
        for id in scene.node_ids_depth_first() {
            if id == humanoid_root {
                continue;
            }
            // Must be under this humanoid: walk parents.
            if !is_descendant(scene, &id, humanoid_root) {
                continue;
            }
            let id_l = id.to_ascii_lowercase();
            if id_l == suffix || id_l.ends_with(&format!("_{suffix}")) || id_l.ends_with(suffix) {
                return Ok(id);
            }
        }
    }
    Err(PoseError::UnknownJoint(
        suffix.into(),
        humanoid_root.into(),
    ))
}

fn is_descendant(scene: &SceneGraph, id: &str, ancestor: &str) -> bool {
    let mut cur = scene.nodes.get(id).and_then(|n| n.parent.clone());
    let mut guard = 0usize;
    while let Some(p) = cur {
        if p == ancestor {
            return true;
        }
        if guard > scene.nodes.len() + 2 {
            return false;
        }
        guard += 1;
        cur = scene.nodes.get(&p).and_then(|n| n.parent.clone());
    }
    false
}

/// Apply a pose op; pushes one undoable batch onto `undo` when provided.
pub fn apply_pose(
    scene: &mut SceneGraph,
    op: &PoseOp,
    undo: Option<&mut UndoStack>,
) -> Result<Vec<SceneOp>, PoseError> {
    let ops = match op {
        PoseOp::RotateJoint {
            humanoid_root,
            joint,
            axis,
            angle_rad,
        } => {
            let id = joint_node_id(scene, humanoid_root, *joint)?;
            vec![rotate_local(scene, &id, *axis, *angle_rad)?]
        }
        PoseOp::LookAt {
            humanoid_root,
            target_world,
        } => {
            let head_id = joint_node_id(scene, humanoid_root, JointId::Head)?;
            vec![look_at_head(scene, humanoid_root, &head_id, *target_world)?]
        }
        PoseOp::Preset {
            humanoid_root,
            preset,
        } => apply_preset(scene, humanoid_root, *preset)?,
    };

    if let Some(stack) = undo {
        // Helpers already mutated the graph; record without re-applying.
        if ops.len() == 1 {
            stack.push_applied(ops[0].clone());
        } else {
            stack.push_applied(SceneOp::Batch { ops: ops.clone() });
        }
    }
    Ok(ops)
}

/// Build SetTransform for a local FK rotation (mutates scene).
fn rotate_local(
    scene: &mut SceneGraph,
    id: &str,
    axis: Vec3,
    angle_rad: f32,
) -> Result<SceneOp, PoseError> {
    let node = scene
        .nodes
        .get(id)
        .ok_or_else(|| PoseError::Scene(format!("unknown node `{id}`")))?;
    let before = node.transform.clone();
    let delta = Quat::from_axis_angle(axis, angle_rad);
    let after = Transform {
        translation: before.translation,
        rotation: (delta * before.rotation)
            .normalized()
            .unwrap_or(before.rotation),
        scale: before.scale,
    };
    scene.set_transform(id, after.clone())?;
    Ok(SceneOp::SetTransform {
        id: id.into(),
        before,
        after,
    })
}

fn set_local_rotation(
    scene: &mut SceneGraph,
    id: &str,
    rotation: Quat,
) -> Result<SceneOp, PoseError> {
    let node = scene
        .nodes
        .get(id)
        .ok_or_else(|| PoseError::Scene(format!("unknown node `{id}`")))?;
    let before = node.transform.clone();
    let after = Transform {
        translation: before.translation,
        rotation: rotation.normalized().unwrap_or(Quat::IDENTITY),
        scale: before.scale,
    };
    scene.set_transform(id, after.clone())?;
    Ok(SceneOp::SetTransform {
        id: id.into(),
        before,
        after,
    })
}

/// IK-lite: yaw (Y) + limited pitch (X) so head faces `target_world`.
fn look_at_head(
    scene: &mut SceneGraph,
    _humanoid_root: &str,
    head_id: &str,
    target_world: Vec3,
) -> Result<SceneOp, PoseError> {
    let head_pos = scene.world_translation(head_id)?;
    let to = target_world - head_pos;
    let flat = Vec3::new(to.x, 0.0, to.z);
    let yaw = if flat.length() < 1e-4 {
        0.0
    } else {
        // Identity forward is −Z; yaw around Y: atan2(x, -z).
        flat.x.atan2(-flat.z)
    };
    let horiz = flat.length().max(1e-4);
    let pitch = (to.y / horiz).atan().clamp(-0.6, 0.6);
    let rot = Quat::from_axis_angle(Vec3::UNIT_Y, yaw) * Quat::from_axis_angle(Vec3::UNIT_X, pitch);
    set_local_rotation(scene, head_id, rot)
}

fn apply_preset(
    scene: &mut SceneGraph,
    humanoid_root: &str,
    preset: PosePreset,
) -> Result<Vec<SceneOp>, PoseError> {
    // Reset limbs to identity first for deterministic presets.
    let mut ops = Vec::new();
    for joint in [
        JointId::Head,
        JointId::ArmL,
        JointId::ArmR,
        JointId::Torso,
    ] {
        if let Ok(id) = joint_node_id(scene, humanoid_root, joint) {
            ops.push(set_local_rotation(scene, &id, Quat::IDENTITY)?);
        }
    }
    match preset {
        PosePreset::Rest => {}
        PosePreset::WaveRight => {
            let id = joint_node_id(scene, humanoid_root, JointId::ArmR)?;
            // Raise arm (local Z) and slight outward.
            ops.push(set_local_rotation(
                scene,
                &id,
                Quat::from_axis_angle(Vec3::UNIT_Z, -1.1)
                    * Quat::from_axis_angle(Vec3::UNIT_X, -0.4),
            )?);
        }
        PosePreset::WaveLeft => {
            let id = joint_node_id(scene, humanoid_root, JointId::ArmL)?;
            ops.push(set_local_rotation(
                scene,
                &id,
                Quat::from_axis_angle(Vec3::UNIT_Z, 1.1)
                    * Quat::from_axis_angle(Vec3::UNIT_X, -0.4),
            )?);
        }
        PosePreset::ReachForward => {
            let id = joint_node_id(scene, humanoid_root, JointId::ArmR)?;
            ops.push(set_local_rotation(
                scene,
                &id,
                Quat::from_axis_angle(Vec3::UNIT_X, -1.2),
            )?);
        }
        PosePreset::LookLeft => {
            let id = joint_node_id(scene, humanoid_root, JointId::Head)?;
            ops.push(set_local_rotation(
                scene,
                &id,
                Quat::from_axis_angle(Vec3::UNIT_Y, 0.7),
            )?);
        }
        PosePreset::LookRight => {
            let id = joint_node_id(scene, humanoid_root, JointId::Head)?;
            ops.push(set_local_rotation(
                scene,
                &id,
                Quat::from_axis_angle(Vec3::UNIT_Y, -0.7),
            )?);
        }
    }
    Ok(ops)
}

/// Convenience: parse preset name and apply.
pub fn apply_pose_preset(
    scene: &mut SceneGraph,
    humanoid_root: &str,
    preset_name: &str,
    undo: Option<&mut UndoStack>,
) -> Result<Vec<SceneOp>, PoseError> {
    let preset =
        PosePreset::parse(preset_name).ok_or_else(|| PoseError::UnknownPreset(preset_name.into()))?;
    apply_pose(
        scene,
        &PoseOp::Preset {
            humanoid_root: humanoid_root.into(),
            preset,
        },
        undo,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::{embedded_primitives_pack, instantiate_asset};
    use crate::scene::SceneGraph;

    fn scene_with_humanoid() -> (SceneGraph, String) {
        let pack = embedded_primitives_pack().expect("pack");
        let scene = SceneGraph::demo_scene();
        // Use demo humanoid root id.
        let root = "humanoid".to_string();
        assert!(scene.nodes.contains_key(&root));
        let _ = pack;
        let _ = instantiate_asset;
        (scene, root)
    }

    #[test]
    fn wave_right_rotates_arm() {
        let (mut scene, root) = scene_with_humanoid();
        let before = scene.nodes["arm_r"].transform.rotation;
        apply_pose_preset(&mut scene, &root, "wave_right", None).expect("pose");
        let after = scene.nodes["arm_r"].transform.rotation;
        assert_ne!(before, after);
        scene.validate().expect("valid");
    }

    #[test]
    fn look_at_changes_head() {
        let (mut scene, root) = scene_with_humanoid();
        let before = scene.nodes["head"].transform.rotation;
        apply_pose(
            &mut scene,
            &PoseOp::LookAt {
                humanoid_root: root,
                target_world: Vec3::new(2.0, 1.5, 0.0),
            },
            None,
        )
        .expect("look");
        assert_ne!(before, scene.nodes["head"].transform.rotation);
    }

    #[test]
    fn undo_restores_wave() {
        let (mut scene, root) = scene_with_humanoid();
        let mut undo = UndoStack::default();
        let before = scene.nodes["arm_r"].transform.clone();
        apply_pose_preset(&mut scene, &root, "wave_right", Some(&mut undo)).expect("pose");
        assert_ne!(before, scene.nodes["arm_r"].transform);
        undo.undo(&mut scene).expect("undo");
        assert_eq!(before, scene.nodes["arm_r"].transform);
    }
}
