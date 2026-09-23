//! Humanoid / quadruped pose: FK, look-at, two-bone IK, pose library.
//!
//! Operates on SceneGraph transforms only (ADR 0011). MeshBox joints remain
//! the visual proxy; beauty backends consume the same posed graph.

use crate::ik::solve_two_bone;
use crate::math::{Quat, Vec3};
use crate::ops::{SceneOp, UndoStack};
use crate::scene::{SceneError, SceneGraph, Transform};
use thiserror::Error;

/// DeclUI / host service id: apply a pose op to a character subtree.
pub const SCENE_POSE_SERVICE: &str = "scene.pose";

/// Cap: apply pose / IK / FK ops (fail-closed).
pub const SCENE_POSE_CAP: &str = "scene.pose";

#[derive(Debug, Error, PartialEq)]
pub enum PoseError {
    #[error("scene: {0}")]
    Scene(String),
    #[error("unknown joint `{0}` under `{1}`")]
    UnknownJoint(String, String),
    #[error("unknown pose preset `{0}`")]
    UnknownPreset(String),
    #[error("unknown IK chain `{0}`")]
    UnknownChain(String),
    #[error("missing character root `{0}`")]
    MissingRoot(String),
    #[error("ik: {0}")]
    Ik(String),
}

impl From<SceneError> for PoseError {
    fn from(e: SceneError) -> Self {
        Self::Scene(e.to_string())
    }
}

/// Named joints relative to a character prefab root (suffix after instance prefix).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JointId {
    Root,
    Pelvis,
    Spine,
    Chest,
    Neck,
    Head,
    /// Legacy single-bone alias → upper arm.
    ArmL,
    ArmR,
    UpperArmL,
    LowerArmL,
    HandL,
    UpperArmR,
    LowerArmR,
    HandR,
    /// Legacy single-bone alias → upper leg.
    LegL,
    LegR,
    UpperLegL,
    LowerLegL,
    FootL,
    UpperLegR,
    LowerLegR,
    FootR,
    // Quadruped
    Body,
    Tail,
    UpperFl,
    LowerFl,
    PawFl,
    UpperFr,
    LowerFr,
    PawFr,
    UpperBl,
    LowerBl,
    PawBl,
    UpperBr,
    LowerBr,
    PawBr,
}

impl JointId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Root => "",
            Self::Pelvis => "pelvis",
            Self::Spine => "spine",
            Self::Chest => "chest",
            Self::Neck => "neck",
            Self::Head => "head",
            Self::ArmL | Self::UpperArmL => "upper_arm_l",
            Self::ArmR | Self::UpperArmR => "upper_arm_r",
            Self::LowerArmL => "lower_arm_l",
            Self::HandL => "hand_l",
            Self::LowerArmR => "lower_arm_r",
            Self::HandR => "hand_r",
            Self::LegL | Self::UpperLegL => "upper_leg_l",
            Self::LegR | Self::UpperLegR => "upper_leg_r",
            Self::LowerLegL => "lower_leg_l",
            Self::FootL => "foot_l",
            Self::LowerLegR => "lower_leg_r",
            Self::FootR => "foot_r",
            Self::Body => "body",
            Self::Tail => "tail",
            Self::UpperFl => "upper_fl",
            Self::LowerFl => "lower_fl",
            Self::PawFl => "paw_fl",
            Self::UpperFr => "upper_fr",
            Self::LowerFr => "lower_fr",
            Self::PawFr => "paw_fr",
            Self::UpperBl => "upper_bl",
            Self::LowerBl => "lower_bl",
            Self::PawBl => "paw_bl",
            Self::UpperBr => "upper_br",
            Self::LowerBr => "lower_br",
            Self::PawBr => "paw_br",
        }
    }

    /// Alternate id suffixes accepted when resolving (legacy MeshBox placeholders).
    fn aliases(self) -> &'static [&'static str] {
        match self {
            Self::ArmL | Self::UpperArmL => &["upper_arm_l", "arm_l"],
            Self::ArmR | Self::UpperArmR => &["upper_arm_r", "arm_r"],
            Self::LegL | Self::UpperLegL => &["upper_leg_l", "leg_l"],
            Self::LegR | Self::UpperLegR => &["upper_leg_r", "leg_r"],
            Self::Chest => &["chest", "torso"],
            Self::Spine => &["spine", "torso"],
            Self::Pelvis => &["pelvis", "hips"],
            Self::Body => &["body", "torso"],
            Self::Root => &[""],
            Self::Neck => &["neck"],
            Self::Head => &["head"],
            Self::LowerArmL => &["lower_arm_l"],
            Self::HandL => &["hand_l"],
            Self::LowerArmR => &["lower_arm_r"],
            Self::HandR => &["hand_r"],
            Self::LowerLegL => &["lower_leg_l"],
            Self::FootL => &["foot_l"],
            Self::LowerLegR => &["lower_leg_r"],
            Self::FootR => &["foot_r"],
            Self::Tail => &["tail"],
            Self::UpperFl => &["upper_fl"],
            Self::LowerFl => &["lower_fl"],
            Self::PawFl => &["paw_fl"],
            Self::UpperFr => &["upper_fr"],
            Self::LowerFr => &["lower_fr"],
            Self::PawFr => &["paw_fr"],
            Self::UpperBl => &["upper_bl"],
            Self::LowerBl => &["lower_bl"],
            Self::PawBl => &["paw_bl"],
            Self::UpperBr => &["upper_br"],
            Self::LowerBr => &["lower_br"],
            Self::PawBr => &["paw_br"],
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "" | "root" | "humanoid" | "character" | "quadruped" => Some(Self::Root),
            "pelvis" | "hips" => Some(Self::Pelvis),
            "spine" => Some(Self::Spine),
            "chest" | "torso" => Some(Self::Chest),
            "neck" => Some(Self::Neck),
            "head" => Some(Self::Head),
            "arm_l" | "arml" | "left_arm" | "upper_arm_l" | "upperarml" => Some(Self::UpperArmL),
            "arm_r" | "armr" | "right_arm" | "upper_arm_r" | "upperarmr" => Some(Self::UpperArmR),
            "lower_arm_l" | "forearm_l" | "elbow_l" => Some(Self::LowerArmL),
            "lower_arm_r" | "forearm_r" | "elbow_r" => Some(Self::LowerArmR),
            "hand_l" | "wrist_l" | "left_hand" => Some(Self::HandL),
            "hand_r" | "wrist_r" | "right_hand" => Some(Self::HandR),
            "leg_l" | "legl" | "left_leg" | "upper_leg_l" | "thigh_l" => Some(Self::UpperLegL),
            "leg_r" | "legr" | "right_leg" | "upper_leg_r" | "thigh_r" => Some(Self::UpperLegR),
            "lower_leg_l" | "shin_l" | "calf_l" => Some(Self::LowerLegL),
            "lower_leg_r" | "shin_r" | "calf_r" => Some(Self::LowerLegR),
            "foot_l" | "ankle_l" | "left_foot" => Some(Self::FootL),
            "foot_r" | "ankle_r" | "right_foot" => Some(Self::FootR),
            "body" => Some(Self::Body),
            "tail" => Some(Self::Tail),
            "upper_fl" | "leg_fl" | "front_left" => Some(Self::UpperFl),
            "lower_fl" => Some(Self::LowerFl),
            "paw_fl" | "foot_fl" => Some(Self::PawFl),
            "upper_fr" | "leg_fr" | "front_right" => Some(Self::UpperFr),
            "lower_fr" => Some(Self::LowerFr),
            "paw_fr" | "foot_fr" => Some(Self::PawFr),
            "upper_bl" | "leg_bl" | "back_left" | "hind_left" => Some(Self::UpperBl),
            "lower_bl" => Some(Self::LowerBl),
            "paw_bl" | "foot_bl" => Some(Self::PawBl),
            "upper_br" | "leg_br" | "back_right" | "hind_right" => Some(Self::UpperBr),
            "lower_br" => Some(Self::LowerBr),
            "paw_br" | "foot_br" => Some(Self::PawBr),
            _ => None,
        }
    }

    /// Joints reset to identity when applying a humanoid preset.
    fn humanoid_reset_set() -> &'static [JointId] {
        &[
            Self::Pelvis,
            Self::Spine,
            Self::Chest,
            Self::Neck,
            Self::Head,
            Self::UpperArmL,
            Self::LowerArmL,
            Self::HandL,
            Self::UpperArmR,
            Self::LowerArmR,
            Self::HandR,
            Self::UpperLegL,
            Self::LowerLegL,
            Self::FootL,
            Self::UpperLegR,
            Self::LowerLegR,
            Self::FootR,
            // Legacy flat placeholders
            Self::ArmL,
            Self::ArmR,
            Self::LegL,
            Self::LegR,
        ]
    }

    fn quadruped_reset_set() -> &'static [JointId] {
        &[
            Self::Body,
            Self::Head,
            Self::Neck,
            Self::Tail,
            Self::UpperFl,
            Self::LowerFl,
            Self::PawFl,
            Self::UpperFr,
            Self::LowerFr,
            Self::PawFr,
            Self::UpperBl,
            Self::LowerBl,
            Self::PawBl,
            Self::UpperBr,
            Self::LowerBr,
            Self::PawBr,
        ]
    }
}

/// Two-bone IK chain ids (effector at tip joint).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IkChain {
    ArmL,
    ArmR,
    LegL,
    LegR,
    FrontL,
    FrontR,
    BackL,
    BackR,
}

impl IkChain {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "arm_l" | "left_arm" | "hand_l" => Some(Self::ArmL),
            "arm_r" | "right_arm" | "hand_r" | "arm" => Some(Self::ArmR),
            "leg_l" | "left_leg" | "foot_l" => Some(Self::LegL),
            "leg_r" | "right_leg" | "foot_r" => Some(Self::LegR),
            "front_l" | "fl" | "leg_fl" => Some(Self::FrontL),
            "front_r" | "fr" | "leg_fr" => Some(Self::FrontR),
            "back_l" | "bl" | "hind_l" | "leg_bl" => Some(Self::BackL),
            "back_r" | "br" | "hind_r" | "leg_br" => Some(Self::BackR),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::ArmL => "arm_l",
            Self::ArmR => "arm_r",
            Self::LegL => "leg_l",
            Self::LegR => "leg_r",
            Self::FrontL => "front_l",
            Self::FrontR => "front_r",
            Self::BackL => "back_l",
            Self::BackR => "back_r",
        }
    }

    fn joints(self) -> (JointId, JointId, JointId) {
        match self {
            Self::ArmL => (JointId::UpperArmL, JointId::LowerArmL, JointId::HandL),
            Self::ArmR => (JointId::UpperArmR, JointId::LowerArmR, JointId::HandR),
            Self::LegL => (JointId::UpperLegL, JointId::LowerLegL, JointId::FootL),
            Self::LegR => (JointId::UpperLegR, JointId::LowerLegR, JointId::FootR),
            Self::FrontL => (JointId::UpperFl, JointId::LowerFl, JointId::PawFl),
            Self::FrontR => (JointId::UpperFr, JointId::LowerFr, JointId::PawFr),
            Self::BackL => (JointId::UpperBl, JointId::LowerBl, JointId::PawBl),
            Self::BackR => (JointId::UpperBr, JointId::LowerBr, JointId::PawBr),
        }
    }
}

/// Pose operation (applied as undoable `SetTransform` batch).
#[derive(Debug, Clone, PartialEq)]
pub enum PoseOp {
    /// Rotate a joint by axis-angle (radians) in local space (FK).
    RotateJoint {
        character_root: String,
        joint: JointId,
        axis: Vec3,
        angle_rad: f32,
    },
    /// Yaw/pitch the head toward a world-space point.
    LookAt {
        character_root: String,
        target_world: Vec3,
    },
    /// Analytic two-bone IK to a world-space target.
    SolveIk {
        character_root: String,
        chain: IkChain,
        target_world: Vec3,
        pole_world: Option<Vec3>,
    },
    /// Named preset applied to a character root.
    Preset {
        character_root: String,
        preset: PosePreset,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PosePreset {
    Rest,
    Standing,
    WaveRight,
    WaveLeft,
    ReachForward,
    Pointing,
    Sitting,
    Lying,
    LookLeft,
    LookRight,
    /// Quadruped: sit / loaf.
    QuadSit,
}

impl PosePreset {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "rest" | "default" | "t_pose" | "tpose" => Some(Self::Rest),
            "standing" | "stand" | "idle" => Some(Self::Standing),
            "wave_right" | "waveright" | "wave" => Some(Self::WaveRight),
            "wave_left" | "waveleft" => Some(Self::WaveLeft),
            "reach_forward" | "reach" => Some(Self::ReachForward),
            "pointing" | "point" => Some(Self::Pointing),
            "sitting" | "sit" | "seated" => Some(Self::Sitting),
            "lying" | "lie" | "prone" | "reclined" => Some(Self::Lying),
            "look_left" | "lookleft" => Some(Self::LookLeft),
            "look_right" | "lookright" => Some(Self::LookRight),
            "quad_sit" | "quadsit" | "cat_sit" | "loaf" => Some(Self::QuadSit),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rest => "rest",
            Self::Standing => "standing",
            Self::WaveRight => "wave_right",
            Self::WaveLeft => "wave_left",
            Self::ReachForward => "reach_forward",
            Self::Pointing => "pointing",
            Self::Sitting => "sitting",
            Self::Lying => "lying",
            Self::LookLeft => "look_left",
            Self::LookRight => "look_right",
            Self::QuadSit => "quad_sit",
        }
    }

    /// All library preset ids (for DeclUI / docs).
    pub fn library() -> &'static [PosePreset] {
        &[
            Self::Rest,
            Self::Standing,
            Self::WaveRight,
            Self::WaveLeft,
            Self::ReachForward,
            Self::Pointing,
            Self::Sitting,
            Self::Lying,
            Self::LookLeft,
            Self::LookRight,
            Self::QuadSit,
        ]
    }
}

/// Resolve node id for a joint under a character instance root.
pub fn joint_node_id(
    scene: &SceneGraph,
    character_root: &str,
    joint: JointId,
) -> Result<String, PoseError> {
    if joint == JointId::Root {
        if scene.nodes.contains_key(character_root) {
            return Ok(character_root.to_string());
        }
        return Err(PoseError::MissingRoot(character_root.into()));
    }
    if !scene.nodes.contains_key(character_root) {
        return Err(PoseError::MissingRoot(character_root.into()));
    }

    for alias in joint.aliases() {
        if alias.is_empty() {
            continue;
        }
        for id in scene.node_ids_depth_first() {
            if id == character_root || !is_descendant(scene, &id, character_root) {
                continue;
            }
            let id_l = id.to_ascii_lowercase();
            if id_l == *alias || id_l.ends_with(&format!("_{alias}")) || id_l.ends_with(alias) {
                return Ok(id);
            }
            if let Some(n) = scene.nodes.get(&id) {
                let name_l = n.name.to_ascii_lowercase().replace(' ', "");
                if name_l == alias.replace('_', "") || name_l == *alias {
                    return Ok(id);
                }
            }
        }
    }
    Err(PoseError::UnknownJoint(
        joint.as_str().into(),
        character_root.into(),
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

fn looks_like_quadruped(scene: &SceneGraph, root: &str) -> bool {
    joint_node_id(scene, root, JointId::UpperFl).is_ok()
        || joint_node_id(scene, root, JointId::Body).is_ok()
}

/// Apply a pose op; pushes one undoable batch onto `undo` when provided.
pub fn apply_pose(
    scene: &mut SceneGraph,
    op: &PoseOp,
    undo: Option<&mut UndoStack>,
) -> Result<Vec<SceneOp>, PoseError> {
    let ops = match op {
        PoseOp::RotateJoint {
            character_root,
            joint,
            axis,
            angle_rad,
        } => {
            let id = joint_node_id(scene, character_root, *joint)?;
            vec![rotate_local(scene, &id, *axis, *angle_rad)?]
        }
        PoseOp::LookAt {
            character_root,
            target_world,
        } => {
            let head_id = joint_node_id(scene, character_root, JointId::Head)?;
            vec![look_at_head(scene, &head_id, *target_world)?]
        }
        PoseOp::SolveIk {
            character_root,
            chain,
            target_world,
            pole_world,
        } => apply_ik(scene, character_root, *chain, *target_world, *pole_world)?,
        PoseOp::Preset {
            character_root,
            preset,
        } => apply_preset(scene, character_root, *preset)?,
    };

    if let Some(stack) = undo {
        if ops.len() == 1 {
            stack.push_applied(ops[0].clone());
        } else if !ops.is_empty() {
            stack.push_applied(SceneOp::Batch { ops: ops.clone() });
        }
    }
    Ok(ops)
}

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

/// Look-at: yaw (Y) + limited pitch (X) so head faces `target_world`.
fn look_at_head(
    scene: &mut SceneGraph,
    head_id: &str,
    target_world: Vec3,
) -> Result<SceneOp, PoseError> {
    let head_pos = scene.world_translation(head_id)?;
    let to = target_world - head_pos;
    let flat = Vec3::new(to.x, 0.0, to.z);
    let yaw = if flat.length() < 1e-4 {
        0.0
    } else {
        flat.x.atan2(-flat.z)
    };
    let horiz = flat.length().max(1e-4);
    let pitch = (to.y / horiz).atan().clamp(-0.6, 0.6);
    let rot = Quat::from_axis_angle(Vec3::UNIT_Y, yaw) * Quat::from_axis_angle(Vec3::UNIT_X, pitch);
    set_local_rotation(scene, head_id, rot)
}

fn apply_ik(
    scene: &mut SceneGraph,
    character_root: &str,
    chain: IkChain,
    target_world: Vec3,
    pole_world: Option<Vec3>,
) -> Result<Vec<SceneOp>, PoseError> {
    let (upper_j, lower_j, tip_j) = chain.joints();
    let upper = joint_node_id(scene, character_root, upper_j)?;
    let lower = joint_node_id(scene, character_root, lower_j)?;
    let tip = joint_node_id(scene, character_root, tip_j)?;

    let solved = solve_two_bone(scene, &upper, &lower, &tip, target_world, pole_world)
        .map_err(|e| PoseError::Ik(e.to_string()))?;

    let ops = vec![
        set_local_rotation(scene, &upper, solved.upper_local)?,
        set_local_rotation(scene, &lower, solved.lower_local)?,
    ];
    Ok(ops)
}

fn reset_joints(
    scene: &mut SceneGraph,
    root: &str,
    joints: &[JointId],
) -> Result<Vec<SceneOp>, PoseError> {
    let mut ops = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for joint in joints {
        if let Ok(id) = joint_node_id(scene, root, *joint) {
            if seen.insert(id.clone()) {
                ops.push(set_local_rotation(scene, &id, Quat::IDENTITY)?);
            }
        }
    }
    Ok(ops)
}

fn apply_preset(
    scene: &mut SceneGraph,
    character_root: &str,
    preset: PosePreset,
) -> Result<Vec<SceneOp>, PoseError> {
    let quad = looks_like_quadruped(scene, character_root);
    let mut ops = if quad {
        reset_joints(scene, character_root, JointId::quadruped_reset_set())?
    } else {
        reset_joints(scene, character_root, JointId::humanoid_reset_set())?
    };

    match preset {
        PosePreset::Rest | PosePreset::Standing => {}
        PosePreset::WaveRight => {
            if let Ok(id) = joint_node_id(scene, character_root, JointId::UpperArmR) {
                ops.push(set_local_rotation(
                    scene,
                    &id,
                    Quat::from_axis_angle(Vec3::UNIT_Z, -1.1)
                        * Quat::from_axis_angle(Vec3::UNIT_X, -0.4),
                )?);
            }
            if let Ok(id) = joint_node_id(scene, character_root, JointId::LowerArmR) {
                ops.push(set_local_rotation(
                    scene,
                    &id,
                    Quat::from_axis_angle(Vec3::UNIT_X, -0.5),
                )?);
            }
        }
        PosePreset::WaveLeft => {
            if let Ok(id) = joint_node_id(scene, character_root, JointId::UpperArmL) {
                ops.push(set_local_rotation(
                    scene,
                    &id,
                    Quat::from_axis_angle(Vec3::UNIT_Z, 1.1)
                        * Quat::from_axis_angle(Vec3::UNIT_X, -0.4),
                )?);
            }
            if let Ok(id) = joint_node_id(scene, character_root, JointId::LowerArmL) {
                ops.push(set_local_rotation(
                    scene,
                    &id,
                    Quat::from_axis_angle(Vec3::UNIT_X, -0.5),
                )?);
            }
        }
        PosePreset::ReachForward => {
            if let Ok(id) = joint_node_id(scene, character_root, JointId::UpperArmR) {
                ops.push(set_local_rotation(
                    scene,
                    &id,
                    Quat::from_axis_angle(Vec3::UNIT_X, -1.2),
                )?);
            }
            if let Ok(id) = joint_node_id(scene, character_root, JointId::LowerArmR) {
                ops.push(set_local_rotation(
                    scene,
                    &id,
                    Quat::from_axis_angle(Vec3::UNIT_X, -0.3),
                )?);
            }
        }
        PosePreset::Pointing => {
            if let Ok(id) = joint_node_id(scene, character_root, JointId::UpperArmR) {
                ops.push(set_local_rotation(
                    scene,
                    &id,
                    Quat::from_axis_angle(Vec3::UNIT_X, -1.35)
                        * Quat::from_axis_angle(Vec3::UNIT_Y, -0.25),
                )?);
            }
            if let Ok(id) = joint_node_id(scene, character_root, JointId::LowerArmR) {
                ops.push(set_local_rotation(
                    scene,
                    &id,
                    Quat::from_axis_angle(Vec3::UNIT_X, -0.15),
                )?);
            }
            if let Ok(id) = joint_node_id(scene, character_root, JointId::Head) {
                ops.push(set_local_rotation(
                    scene,
                    &id,
                    Quat::from_axis_angle(Vec3::UNIT_Y, -0.2),
                )?);
            }
        }
        PosePreset::Sitting => {
            if let Ok(id) = joint_node_id(scene, character_root, JointId::Pelvis) {
                ops.push(set_local_rotation(
                    scene,
                    &id,
                    Quat::from_axis_angle(Vec3::UNIT_X, -0.35),
                )?);
            }
            for joint in [JointId::UpperLegL, JointId::UpperLegR] {
                if let Ok(id) = joint_node_id(scene, character_root, joint) {
                    ops.push(set_local_rotation(
                        scene,
                        &id,
                        Quat::from_axis_angle(Vec3::UNIT_X, 1.2),
                    )?);
                }
            }
            for joint in [JointId::LowerLegL, JointId::LowerLegR] {
                if let Ok(id) = joint_node_id(scene, character_root, joint) {
                    ops.push(set_local_rotation(
                        scene,
                        &id,
                        Quat::from_axis_angle(Vec3::UNIT_X, -1.35),
                    )?);
                }
            }
            for joint in [JointId::UpperArmL, JointId::UpperArmR] {
                if let Ok(id) = joint_node_id(scene, character_root, joint) {
                    let sign = if joint == JointId::UpperArmL {
                        1.0
                    } else {
                        -1.0
                    };
                    ops.push(set_local_rotation(
                        scene,
                        &id,
                        Quat::from_axis_angle(Vec3::UNIT_Z, 0.35 * sign)
                            * Quat::from_axis_angle(Vec3::UNIT_X, -0.4),
                    )?);
                }
            }
        }
        PosePreset::Lying => {
            if let Ok(id) = joint_node_id(scene, character_root, JointId::Pelvis) {
                ops.push(set_local_rotation(
                    scene,
                    &id,
                    Quat::from_axis_angle(Vec3::UNIT_Z, 1.45),
                )?);
            } else if let Ok(id) = joint_node_id(scene, character_root, JointId::Root) {
                ops.push(set_local_rotation(
                    scene,
                    &id,
                    Quat::from_axis_angle(Vec3::UNIT_Z, 1.45),
                )?);
            }
            for joint in [JointId::UpperLegL, JointId::UpperLegR] {
                if let Ok(id) = joint_node_id(scene, character_root, joint) {
                    ops.push(set_local_rotation(
                        scene,
                        &id,
                        Quat::from_axis_angle(Vec3::UNIT_X, 0.4),
                    )?);
                }
            }
        }
        PosePreset::LookLeft => {
            if let Ok(id) = joint_node_id(scene, character_root, JointId::Head) {
                ops.push(set_local_rotation(
                    scene,
                    &id,
                    Quat::from_axis_angle(Vec3::UNIT_Y, 0.7),
                )?);
            }
        }
        PosePreset::LookRight => {
            if let Ok(id) = joint_node_id(scene, character_root, JointId::Head) {
                ops.push(set_local_rotation(
                    scene,
                    &id,
                    Quat::from_axis_angle(Vec3::UNIT_Y, -0.7),
                )?);
            }
        }
        PosePreset::QuadSit => {
            if let Ok(id) = joint_node_id(scene, character_root, JointId::Body) {
                ops.push(set_local_rotation(
                    scene,
                    &id,
                    Quat::from_axis_angle(Vec3::UNIT_X, -0.15),
                )?);
            }
            for joint in [JointId::UpperBl, JointId::UpperBr] {
                if let Ok(id) = joint_node_id(scene, character_root, joint) {
                    ops.push(set_local_rotation(
                        scene,
                        &id,
                        Quat::from_axis_angle(Vec3::UNIT_X, 0.9),
                    )?);
                }
            }
            for joint in [JointId::LowerBl, JointId::LowerBr] {
                if let Ok(id) = joint_node_id(scene, character_root, joint) {
                    ops.push(set_local_rotation(
                        scene,
                        &id,
                        Quat::from_axis_angle(Vec3::UNIT_X, -1.2),
                    )?);
                }
            }
            if let Ok(id) = joint_node_id(scene, character_root, JointId::Tail) {
                ops.push(set_local_rotation(
                    scene,
                    &id,
                    Quat::from_axis_angle(Vec3::UNIT_X, 0.4),
                )?);
            }
        }
    }
    Ok(ops)
}

/// Convenience: parse preset name and apply.
pub fn apply_pose_preset(
    scene: &mut SceneGraph,
    character_root: &str,
    preset_name: &str,
    undo: Option<&mut UndoStack>,
) -> Result<Vec<SceneOp>, PoseError> {
    let preset = PosePreset::parse(preset_name)
        .ok_or_else(|| PoseError::UnknownPreset(preset_name.into()))?;
    apply_pose(
        scene,
        &PoseOp::Preset {
            character_root: character_root.into(),
            preset,
        },
        undo,
    )
}

/// Convenience: two-bone IK by chain name.
pub fn apply_ik_chain(
    scene: &mut SceneGraph,
    character_root: &str,
    chain_name: &str,
    target_world: Vec3,
    pole_world: Option<Vec3>,
    undo: Option<&mut UndoStack>,
) -> Result<Vec<SceneOp>, PoseError> {
    let chain =
        IkChain::parse(chain_name).ok_or_else(|| PoseError::UnknownChain(chain_name.into()))?;
    apply_pose(
        scene,
        &PoseOp::SolveIk {
            character_root: character_root.into(),
            chain,
            target_world,
            pole_world,
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
        let scene = SceneGraph::demo_scene();
        let root = "humanoid".to_string();
        assert!(scene.nodes.contains_key(&root));
        (scene, root)
    }

    #[test]
    fn wave_right_rotates_upper_arm() {
        let (mut scene, root) = scene_with_humanoid();
        let arm = joint_node_id(&scene, &root, JointId::UpperArmR).expect("arm");
        let before = scene.nodes[&arm].transform.rotation;
        apply_pose_preset(&mut scene, &root, "wave_right", None).expect("pose");
        let after = scene.nodes[&arm].transform.rotation;
        assert_ne!(before, after);
        scene.validate().expect("valid");
    }

    #[test]
    fn look_at_changes_head() {
        let (mut scene, root) = scene_with_humanoid();
        let head = joint_node_id(&scene, &root, JointId::Head).expect("head");
        let before = scene.nodes[&head].transform.rotation;
        apply_pose(
            &mut scene,
            &PoseOp::LookAt {
                character_root: root,
                target_world: Vec3::new(2.0, 1.5, 0.0),
            },
            None,
        )
        .expect("look");
        assert_ne!(before, scene.nodes[&head].transform.rotation);
    }

    #[test]
    fn undo_restores_wave() {
        let (mut scene, root) = scene_with_humanoid();
        let mut undo = UndoStack::default();
        let arm = joint_node_id(&scene, &root, JointId::UpperArmR).expect("arm");
        let before = scene.nodes[&arm].transform.clone();
        apply_pose_preset(&mut scene, &root, "wave_right", Some(&mut undo)).expect("pose");
        assert_ne!(before, scene.nodes[&arm].transform);
        undo.undo(&mut scene).expect("undo");
        assert_eq!(before, scene.nodes[&arm].transform);
    }

    #[test]
    fn sitting_preset_bends_legs() {
        let (mut scene, root) = scene_with_humanoid();
        apply_pose_preset(&mut scene, &root, "sitting", None).expect("sit");
        let thigh = joint_node_id(&scene, &root, JointId::UpperLegR).expect("thigh");
        assert_ne!(scene.nodes[&thigh].transform.rotation, Quat::IDENTITY);
    }

    #[test]
    fn arm_ik_moves_hand_toward_target() {
        let (mut scene, root) = scene_with_humanoid();
        let hand = joint_node_id(&scene, &root, JointId::HandR).expect("hand");
        let before = scene.world_translation(&hand).expect("pos");
        let target = Vec3::new(before.x + 0.35, before.y + 0.2, before.z - 0.15);
        apply_ik_chain(
            &mut scene,
            &root,
            "arm_r",
            target,
            Some(Vec3::new(before.x, before.y + 1.0, before.z + 1.0)),
            None,
        )
        .expect("ik");
        let after = scene.world_translation(&hand).expect("pos2");
        let before_err = (before - target).length();
        let after_err = (after - target).length();
        assert!(
            after_err < before_err - 0.05,
            "IK should reduce error: before={before_err} after={after_err}"
        );
    }

    #[test]
    fn quadruped_pack_instantiates_and_sits() {
        let pack = embedded_primitives_pack().expect("pack");
        assert!(pack.get("quadruped.cat").is_some());
        let mut scene = SceneGraph::demo_scene();
        let r = instantiate_asset(&mut scene, &pack, "quadruped.cat", Some("root"), "cat_")
            .expect("cat");
        apply_pose_preset(&mut scene, &r.root_id, "quad_sit", None).expect("sit");
        scene.validate().expect("valid");
    }

    #[test]
    fn articulated_humanoid_pack_has_hand_joints() {
        let pack = embedded_primitives_pack().expect("pack");
        let mut scene = SceneGraph {
            nodes: {
                let mut m = std::collections::HashMap::new();
                let root = crate::scene::SceneNode::empty("root", "Scene");
                m.insert(root.id.clone(), root);
                m
            },
            roots: vec!["root".into()],
            active_camera: None,
            effects: Vec::new(),
        };
        let r = instantiate_asset(
            &mut scene,
            &pack,
            "humanoid.placeholder",
            Some("root"),
            "h_",
        )
        .expect("h");
        assert!(joint_node_id(&scene, &r.root_id, JointId::HandR).is_ok());
        assert!(joint_node_id(&scene, &r.root_id, JointId::LowerArmL).is_ok());
        assert!(joint_node_id(&scene, &r.root_id, JointId::Pelvis).is_ok());
    }
}
