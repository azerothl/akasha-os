//! Analytic two-bone IK (ADR 0011).
//!
//! Bones rest along the child-joint translation axis (typically local **−Y**).
//! The solver works in the **upper joint's parent space**, then returns local
//! quaternions for the upper and lower joints.

use crate::math::{Mat4, Quat, Vec3, EPSILON};
use crate::scene::{SceneError, SceneGraph};

/// Result of a two-bone IK solve (local rotations).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TwoBoneIkResult {
    pub upper_local: Quat,
    pub lower_local: Quat,
    /// Predicted tip in world space after applying the local rotations.
    pub tip_world: Vec3,
    /// Distance from predicted tip to the original target.
    pub tip_error: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IkError {
    Scene(String),
    DegenerateChain,
}

impl From<SceneError> for IkError {
    fn from(e: SceneError) -> Self {
        Self::Scene(e.to_string())
    }
}

impl std::fmt::Display for IkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Scene(s) => write!(f, "scene: {s}"),
            Self::DegenerateChain => write!(f, "degenerate IK chain"),
        }
    }
}

impl std::error::Error for IkError {}

/// Analytic two-bone IK → local quaternions for `upper_id` / `lower_id`.
pub fn solve_two_bone(
    scene: &SceneGraph,
    upper_id: &str,
    lower_id: &str,
    tip_id: &str,
    target_world: Vec3,
    pole_world: Option<Vec3>,
) -> Result<TwoBoneIkResult, IkError> {
    let upper = scene
        .nodes
        .get(upper_id)
        .ok_or_else(|| IkError::Scene(format!("unknown upper `{upper_id}`")))?;
    let lower = scene
        .nodes
        .get(lower_id)
        .ok_or_else(|| IkError::Scene(format!("unknown lower `{lower_id}`")))?;
    let tip = scene
        .nodes
        .get(tip_id)
        .ok_or_else(|| IkError::Scene(format!("unknown tip `{tip_id}`")))?;

    let len1 = lower.transform.translation.length().max(EPSILON);
    let len2 = tip.transform.translation.length().max(EPSILON);
    let rest_upper = lower
        .transform
        .translation
        .normalized()
        .ok_or(IkError::DegenerateChain)?;
    let rest_lower = tip
        .transform
        .translation
        .normalized()
        .ok_or(IkError::DegenerateChain)?;

    let parent_world = match &upper.parent {
        Some(p) => scene.world_matrix(p)?,
        None => Mat4::IDENTITY,
    };
    let parent_inv = parent_world
        .try_inverse_affine()
        .ok_or(IkError::DegenerateChain)?;

    // Solve in the upper joint's parent space.
    let root = parent_inv.transform_point(scene.world_translation(upper_id)?);
    let target_p = parent_inv.transform_point(target_world);
    let mid_p = parent_inv.transform_point(scene.world_translation(lower_id)?);
    let pole_p = pole_world.map(|p| parent_inv.transform_point(p)).unwrap_or_else(|| {
        let dir_guess = (target_p - root)
            .normalized()
            .unwrap_or(Vec3::new(0.0, -1.0, 0.0));
        let lateral = (mid_p - root) - dir_guess * (mid_p - root).dot(dir_guess);
        if lateral.length_squared() > 1e-6 {
            mid_p
        } else {
            root + Vec3::UNIT_X
        }
    });

    let mut to_target = target_p - root;
    let mut dist = to_target.length();
    let max_reach = (len1 + len2 - 1e-3).max(EPSILON);
    let min_reach = ((len1 - len2).abs() + 1e-3).min(max_reach);
    if dist < EPSILON {
        to_target = rest_upper;
        dist = EPSILON;
    }
    let clamped = dist.clamp(min_reach, max_reach);
    let dir = to_target.normalized().unwrap_or(rest_upper);
    let target_clamped = root + dir * clamped;

    let mut pole_dir = pole_p - root;
    pole_dir = pole_dir - dir * pole_dir.dot(dir);
    let bend_normal = dir
        .cross(pole_dir.normalized().unwrap_or(Vec3::UNIT_X))
        .normalized()
        .unwrap_or_else(|| {
            dir.cross(Vec3::UNIT_Y)
                .normalized()
                .or_else(|| dir.cross(Vec3::UNIT_X).normalized())
                .unwrap_or(Vec3::UNIT_Z)
        });

    let cos_root = ((len1 * len1 + clamped * clamped - len2 * len2)
        / (2.0 * len1 * clamped.max(EPSILON)))
    .clamp(-1.0, 1.0);
    let sin_root = (1.0 - cos_root * cos_root).max(0.0).sqrt();
    // Rotate `dir` toward the bend plane by ±acos(cos_root).
    let upper_dir = (dir * cos_root + bend_normal.cross(dir) * sin_root)
        .normalized()
        .unwrap_or(dir);
    let elbow = root + upper_dir * len1;
    let lower_dir = (target_clamped - elbow)
        .normalized()
        .unwrap_or(upper_dir);

    // Local rotations in parent / upper frames (scale ignored for orientation).
    let upper_local = Quat::from_rotation_arc(rest_upper, upper_dir);
    let lower_dir_in_upper = upper_local.conjugate().rotate_vec(lower_dir);
    let lower_local = Quat::from_rotation_arc(rest_lower, lower_dir_in_upper);

    let tip_parent = elbow + lower_dir * len2;
    let tip_world = parent_world.transform_point(tip_parent);
    let tip_error = (tip_world - target_world).length();

    Ok(TwoBoneIkResult {
        upper_local,
        lower_local,
        tip_world,
        tip_error,
    })
}

/// Unit-test fixture: two-bone chain under `root`.
#[cfg(test)]
pub fn fixture_two_bone_scene() -> (SceneGraph, &'static str, &'static str, &'static str) {
    use crate::scene::{NodeKind, SceneNode, Transform};
    use std::collections::HashMap;

    let mut nodes = HashMap::new();
    let mut root = SceneNode::empty("root", "Root");
    root.children = vec!["upper".into()];

    let mut upper = SceneNode::empty("upper", "Upper");
    upper.kind = NodeKind::MeshBox;
    upper.parent = Some("root".into());
    upper.children = vec!["lower".into()];
    upper.transform = Transform {
        translation: Vec3::new(0.0, 1.0, 0.0),
        rotation: Quat::IDENTITY,
        scale: Vec3::ONE,
    };

    let mut lower = SceneNode::empty("lower", "Lower");
    lower.kind = NodeKind::MeshBox;
    lower.parent = Some("upper".into());
    lower.children = vec!["tip".into()];
    lower.transform = Transform {
        translation: Vec3::new(0.0, -0.4, 0.0),
        rotation: Quat::IDENTITY,
        scale: Vec3::ONE,
    };

    let mut tip = SceneNode::empty("tip", "Tip");
    tip.kind = NodeKind::MeshBox;
    tip.parent = Some("lower".into());
    tip.transform = Transform {
        translation: Vec3::new(0.0, -0.35, 0.0),
        rotation: Quat::IDENTITY,
        scale: Vec3::ONE,
    };

    for n in [root, upper, lower, tip] {
        nodes.insert(n.id.clone(), n);
    }
    let scene = SceneGraph {
        nodes,
        roots: vec!["root".into()],
        active_camera: None,
    };
    (scene, "upper", "lower", "tip")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Transform;

    #[test]
    fn two_bone_reaches_reachable_target() {
        let (mut scene, upper, lower, tip) = fixture_two_bone_scene();
        let target = Vec3::new(0.5, 0.6, 0.0);
        let solved =
            solve_two_bone(&scene, upper, lower, tip, target, Some(Vec3::new(0.0, 1.0, 1.0)))
                .expect("ik");
        let u = scene.nodes[upper].transform.clone();
        scene
            .set_transform(
                upper,
                Transform {
                    translation: u.translation,
                    rotation: solved.upper_local,
                    scale: u.scale,
                },
            )
            .unwrap();
        let l = scene.nodes[lower].transform.clone();
        scene
            .set_transform(
                lower,
                Transform {
                    translation: l.translation,
                    rotation: solved.lower_local,
                    scale: l.scale,
                },
            )
            .unwrap();
        let tip_pos = scene.world_translation(tip).unwrap();
        let err = (tip_pos - target).length();
        assert!(
            err < 0.08,
            "tip error {err} (tip={tip_pos:?} target={target:?} solver_pred_err={})",
            solved.tip_error
        );
    }

    #[test]
    fn two_bone_clamps_beyond_reach() {
        let (scene, upper, lower, tip) = fixture_two_bone_scene();
        let target = Vec3::new(10.0, 1.0, 0.0);
        let solved = solve_two_bone(&scene, upper, lower, tip, target, None).expect("ik");
        let root = scene.world_translation(upper).unwrap();
        let reach = (solved.tip_world - root).length();
        assert!(reach < 0.8 && reach > 0.6, "reach={reach}");
    }
}
