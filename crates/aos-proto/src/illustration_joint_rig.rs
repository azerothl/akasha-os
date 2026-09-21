//! Human joint rig. Bone lengths come from the standing canon, never from the
//! skeleton being edited. One driver joint rebuilds the rest: the torso moves
//! as a rigid block (the head goes with it), a limb is a two-bone chain, and
//! rotating the head does not change any joint position.

use crate::illustration_taxonomy::{pose_instance_for, ClassificationResult};
use crate::IllustrationSkeletonJoint;
use std::collections::HashMap;

/// How a named segment reacts when another joint moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JointSegmentRule {
    /// Keeps its offset from the pelvis. Moving the torso moves the whole block.
    Rigid,
    /// Bend joint. Its position is solved from a locked root and a target.
    TwoBone,
    /// Orientation only. Pitch, yaw, and roll do not change `x` or `y`.
    RotationOnly,
}

/// What stays put for a given edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JointLock {
    Ground,
    Seat,
    Shoulder,
}

/// A pose change the rig knows how to propagate.
#[derive(Debug, Clone, PartialEq)]
pub enum HumanJointEdit {
    Move { joint: String, x: f32, y: f32 },
    RotateHead { pitch: f32, yaw: f32, roll: f32 },
}

const RIGID: &[&str] = &[
    "pelvis",
    "waist",
    "chest",
    "neck",
    "head",
    "shoulder_l",
    "shoulder_r",
    "hip_l",
    "hip_r",
];
const DRIVERS: &[&str] = &["pelvis", "hand_l", "hand_r", "foot_l", "foot_r"];
const HUMAN: &[&str] = &[
    "pelvis",
    "waist",
    "chest",
    "neck",
    "head",
    "shoulder_l",
    "elbow_l",
    "hand_l",
    "shoulder_r",
    "elbow_r",
    "hand_r",
    "hip_l",
    "knee_l",
    "ankle_l",
    "foot_l",
    "hip_r",
    "knee_r",
    "ankle_r",
    "foot_r",
];

pub fn human_segment_rule(joint_id: &str) -> Option<JointSegmentRule> {
    match joint_id {
        "pelvis" | "waist" | "chest" | "neck" | "shoulder_l" | "shoulder_r" | "hip_l" | "hip_r" => {
            Some(JointSegmentRule::Rigid)
        }
        "head" => Some(JointSegmentRule::RotationOnly),
        "elbow_l" | "hand_l" | "elbow_r" | "hand_r" | "knee_l" | "ankle_l" | "foot_l"
        | "knee_r" | "ankle_r" | "foot_r" => Some(JointSegmentRule::TwoBone),
        _ => None,
    }
}

pub fn human_bone_length(child_id: &str) -> Option<f32> {
    canon_lengths().get(child_id).copied()
}

pub fn human_joint_locks(edit: &HumanJointEdit) -> Vec<(&str, JointLock)> {
    match edit {
        HumanJointEdit::Move { joint, .. } if joint == "pelvis" => {
            vec![("foot_l", JointLock::Ground), ("foot_r", JointLock::Ground)]
        }
        HumanJointEdit::Move { joint, .. } if joint == "hand_l" => {
            vec![
                ("shoulder_l", JointLock::Shoulder),
                ("pelvis", JointLock::Seat),
            ]
        }
        HumanJointEdit::Move { joint, .. } if joint == "hand_r" => {
            vec![
                ("shoulder_r", JointLock::Shoulder),
                ("pelvis", JointLock::Seat),
            ]
        }
        HumanJointEdit::Move { joint, .. } if joint == "foot_l" => {
            vec![("pelvis", JointLock::Seat), ("hip_l", JointLock::Seat)]
        }
        HumanJointEdit::Move { joint, .. } if joint == "foot_r" => {
            vec![("pelvis", JointLock::Seat), ("hip_r", JointLock::Seat)]
        }
        HumanJointEdit::Move { .. } | HumanJointEdit::RotateHead { .. } => Vec::new(),
    }
}

/// Apply `edit` using canon lengths. On error the slice is left unchanged.
pub fn propagate_human_joint_edit(
    skeleton: &mut [IllustrationSkeletonJoint],
    edit: &HumanJointEdit,
) -> Result<(), String> {
    let mut next = skeleton.to_vec();
    apply_edit(&mut next, edit)?;
    for (dst, src) in skeleton.iter_mut().zip(next) {
        *dst = src;
    }
    Ok(())
}

/// If exactly one driver (pelvis, hand, or foot) moved, rebuild from `previous`
/// with canon lengths. Several drivers means a newly authored pose: return `None`.
pub fn reconcile_human_skeleton(
    previous: &[IllustrationSkeletonJoint],
    edited: &[IllustrationSkeletonJoint],
) -> Result<Option<Vec<IllustrationSkeletonJoint>>, String> {
    if !is_human_rig(previous) || !is_human_rig(edited) {
        return Ok(None);
    }
    let moved: Vec<&str> = DRIVERS
        .iter()
        .copied()
        .filter(|id| {
            let a = pos(previous, id).unwrap_or((0.0, 0.0));
            let b = pos(edited, id).unwrap_or((0.0, 0.0));
            (a.0 - b.0).hypot(a.1 - b.1) > 0.0001
        })
        .collect();
    match moved.as_slice() {
        [joint] => {
            let (x, y) = pos(edited, joint)?;
            let mut next = previous.to_vec();
            propagate_human_joint_edit(
                &mut next,
                &HumanJointEdit::Move {
                    joint: (*joint).to_string(),
                    x,
                    y,
                },
            )?;
            Ok(Some(next))
        }
        _ => Ok(None),
    }
}

fn apply_edit(
    skeleton: &mut [IllustrationSkeletonJoint],
    edit: &HumanJointEdit,
) -> Result<(), String> {
    match edit {
        HumanJointEdit::RotateHead { pitch, yaw, roll } => {
            rotate_head(skeleton, *pitch, *yaw, *roll)
        }
        HumanJointEdit::Move { joint, x, y } => {
            if !x.is_finite() || !y.is_finite() {
                return Err(format!("{joint}: non-finite position"));
            }
            match joint.as_str() {
                "pelvis" => move_torso(skeleton, *x, *y),
                "hand_l" => move_hand(skeleton, "shoulder_l", "elbow_l", "hand_l", *x, *y),
                "hand_r" => move_hand(skeleton, "shoulder_r", "elbow_r", "hand_r", *x, *y),
                "foot_l" => move_foot(skeleton, "hip_l", "knee_l", "ankle_l", "foot_l", *x, *y),
                "foot_r" => move_foot(skeleton, "hip_r", "knee_r", "ankle_r", "foot_r", *x, *y),
                "head" => Err("head follows the torso; use RotateHead for orientation".into()),
                other if human_segment_rule(other).is_some() => Err(format!(
                    "{other} is solved by the rig; move the pelvis, a hand, or a foot"
                )),
                other => Err(format!("{other}: not a human rig joint")),
            }
        }
    }
}

fn rotate_head(
    skeleton: &mut [IllustrationSkeletonJoint],
    pitch: f32,
    yaw: f32,
    roll: f32,
) -> Result<(), String> {
    if !pitch.is_finite() || !yaw.is_finite() || !roll.is_finite() {
        return Err("head rotation is non-finite".into());
    }
    let before: Vec<(f32, f32)> = skeleton.iter().map(|j| (j.x, j.y)).collect();
    let head = joint_mut(skeleton, "head")?;
    head.pitch = pitch;
    head.yaw = yaw;
    head.roll = roll;
    for (joint, (x, y)) in skeleton.iter().zip(before) {
        if (joint.x - x).abs() > f32::EPSILON || (joint.y - y).abs() > f32::EPSILON {
            return Err("head rotation moved a joint".into());
        }
    }
    Ok(())
}

fn move_torso(skeleton: &mut [IllustrationSkeletonJoint], x: f32, y: f32) -> Result<(), String> {
    let (px, py) = pos(skeleton, "pelvis")?;
    let (dx, dy) = (x - px, y - py);
    let legs = [
        snap_leg(skeleton, "hip_l", "knee_l", "ankle_l", "foot_l")?,
        snap_leg(skeleton, "hip_r", "knee_r", "ankle_r", "foot_r")?,
    ];
    for id in RIGID
        .iter()
        .chain(["elbow_l", "hand_l", "elbow_r", "hand_r"].iter())
    {
        let joint = joint_mut(skeleton, id)?;
        joint.x += dx;
        joint.y += dy;
    }
    let lengths = canon_lengths();
    for leg in &legs {
        solve_leg(skeleton, leg, &lengths)?;
    }
    Ok(())
}

fn move_hand(
    skeleton: &mut [IllustrationSkeletonJoint],
    shoulder: &str,
    elbow: &str,
    hand: &str,
    x: f32,
    y: f32,
) -> Result<(), String> {
    let root = pos(skeleton, shoulder)?;
    let side = bend_side(root, pos(skeleton, elbow)?, pos(skeleton, hand)?);
    let lengths = canon_lengths();
    let bend = solve_two_bone(
        root,
        (x, y),
        length(&lengths, elbow)?,
        length(&lengths, hand)?,
        side,
        hand,
    )?;
    set_pos(skeleton, elbow, bend)?;
    set_pos(skeleton, hand, (x, y))?;
    Ok(())
}

fn move_foot(
    skeleton: &mut [IllustrationSkeletonJoint],
    hip: &str,
    knee: &str,
    ankle: &str,
    foot: &str,
    x: f32,
    y: f32,
) -> Result<(), String> {
    let hip_pos = pos(skeleton, hip)?;
    let ankle_pos = pos(skeleton, ankle)?;
    let foot_pos = pos(skeleton, foot)?;
    let side = bend_side(hip_pos, pos(skeleton, knee)?, ankle_pos);
    let lengths = canon_lengths();
    let foot_len = length(&lengths, foot)?;
    let (ux, uy) = foot_direction(ankle_pos, foot_pos);
    let ankle_target = (x - ux * foot_len, y - uy * foot_len);
    let bend = solve_two_bone(
        hip_pos,
        ankle_target,
        length(&lengths, knee)?,
        length(&lengths, ankle)?,
        side,
        foot,
    )?;
    set_pos(skeleton, knee, bend)?;
    set_pos(skeleton, ankle, ankle_target)?;
    set_pos(skeleton, foot, (x, y))?;
    Ok(())
}

struct LegSnap {
    hip: &'static str,
    knee: &'static str,
    ankle: &'static str,
    foot: &'static str,
    foot_pos: (f32, f32),
    ankle_pos: (f32, f32),
    side: f32,
}

fn snap_leg(
    skeleton: &[IllustrationSkeletonJoint],
    hip: &'static str,
    knee: &'static str,
    ankle: &'static str,
    foot: &'static str,
) -> Result<LegSnap, String> {
    let hip_pos = pos(skeleton, hip)?;
    let ankle_pos = pos(skeleton, ankle)?;
    Ok(LegSnap {
        hip,
        knee,
        ankle,
        foot,
        foot_pos: pos(skeleton, foot)?,
        ankle_pos,
        side: bend_side(hip_pos, pos(skeleton, knee)?, ankle_pos),
    })
}

fn solve_leg(
    skeleton: &mut [IllustrationSkeletonJoint],
    leg: &LegSnap,
    lengths: &HashMap<String, f32>,
) -> Result<(), String> {
    let hip = pos(skeleton, leg.hip)?;
    let foot_len = length(lengths, leg.foot)?;
    let (ux, uy) = foot_direction(leg.ankle_pos, leg.foot_pos);
    let ankle_target = (
        leg.foot_pos.0 - ux * foot_len,
        leg.foot_pos.1 - uy * foot_len,
    );
    let bend = solve_two_bone(
        hip,
        ankle_target,
        length(lengths, leg.knee)?,
        length(lengths, leg.ankle)?,
        leg.side,
        leg.foot,
    )?;
    set_pos(skeleton, leg.knee, bend)?;
    set_pos(skeleton, leg.ankle, ankle_target)?;
    set_pos(skeleton, leg.foot, leg.foot_pos)?;
    Ok(())
}

fn solve_two_bone(
    root: (f32, f32),
    target: (f32, f32),
    l1: f32,
    l2: f32,
    side: f32,
    name: &str,
) -> Result<(f32, f32), String> {
    let (dx, dy) = (target.0 - root.0, target.1 - root.1);
    let d = dx.hypot(dy);
    if l1 < 0.00001
        || l2 < 0.00001
        || d < 0.00001
        || d > l1 + l2 + 0.000001
        || d < (l1 - l2).abs() - 0.000001
    {
        return Err(format!(
            "{name}: target unreachable without changing limb lengths"
        ));
    }
    let along = (l1 * l1 - l2 * l2 + d * d) / (2.0 * d);
    let height = (l1 * l1 - along * along).max(0.0).sqrt();
    let bend_x = root.0 + along * dx / d - side * height * dy / d;
    let bend_y = root.1 + along * dy / d + side * height * dx / d;
    Ok((bend_x, bend_y))
}

fn bend_side(root: (f32, f32), bend: (f32, f32), target: (f32, f32)) -> f32 {
    let dx = target.0 - root.0;
    let dy = target.1 - root.1;
    let cross = dx * (bend.1 - root.1) - dy * (bend.0 - root.0);
    if cross < 0.0 {
        -1.0
    } else {
        1.0
    }
}

fn unit(dx: f32, dy: f32) -> Option<(f32, f32)> {
    let d = dx.hypot(dy);
    if d < 0.00001 {
        None
    } else {
        Some((dx / d, dy / d))
    }
}

fn foot_direction(ankle: (f32, f32), foot: (f32, f32)) -> (f32, f32) {
    unit(foot.0 - ankle.0, foot.1 - ankle.1)
        .unwrap_or_else(|| unit(0.028, 0.045).expect("fallback foot direction"))
}

fn canon_lengths() -> HashMap<String, f32> {
    let class = ClassificationResult {
        family_id: "humanoid".into(),
        pose_id: Some("human_standing".into()),
        ..ClassificationResult::default()
    };
    let joints = pose_instance_for(&class)
        .map(|pose| pose.skeleton)
        .unwrap_or_default();
    let mut lengths = HashMap::new();
    for joint in &joints {
        let Some(parent) = joint.parent.as_deref() else {
            continue;
        };
        let Some(origin) = joints.iter().find(|other| other.id == parent) else {
            continue;
        };
        lengths.insert(
            joint.id.clone(),
            (joint.x - origin.x).hypot(joint.y - origin.y),
        );
    }
    lengths
}

fn length(lengths: &HashMap<String, f32>, id: &str) -> Result<f32, String> {
    lengths
        .get(id)
        .copied()
        .ok_or_else(|| format!("missing canon length for {id}"))
}

fn is_human_rig(skeleton: &[IllustrationSkeletonJoint]) -> bool {
    HUMAN
        .iter()
        .all(|id| skeleton.iter().any(|joint| joint.id == *id))
}

fn pos(skeleton: &[IllustrationSkeletonJoint], id: &str) -> Result<(f32, f32), String> {
    skeleton
        .iter()
        .find(|joint| joint.id == id)
        .map(|joint| (joint.x, joint.y))
        .ok_or_else(|| format!("missing joint {id}"))
}

fn joint_mut<'a>(
    skeleton: &'a mut [IllustrationSkeletonJoint],
    id: &str,
) -> Result<&'a mut IllustrationSkeletonJoint, String> {
    skeleton
        .iter_mut()
        .find(|joint| joint.id == id)
        .ok_or_else(|| format!("missing joint {id}"))
}

fn set_pos(
    skeleton: &mut [IllustrationSkeletonJoint],
    id: &str,
    at: (f32, f32),
) -> Result<(), String> {
    let joint = joint_mut(skeleton, id)?;
    joint.x = at.0;
    joint.y = at.1;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn standing() -> Vec<IllustrationSkeletonJoint> {
        let class = ClassificationResult {
            family_id: "humanoid".into(),
            pose_id: Some("human_standing".into()),
            ..ClassificationResult::default()
        };
        pose_instance_for(&class).expect("standing").skeleton
    }

    fn at(skeleton: &[IllustrationSkeletonJoint], id: &str) -> (f32, f32) {
        pos(skeleton, id).unwrap_or_else(|_| panic!("missing {id}"))
    }

    fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
        (a.0 - b.0).hypot(a.1 - b.1)
    }

    #[test]
    fn pelvis_move_keeps_feet_and_canon_legs() {
        let mut skeleton = standing();
        let feet = (at(&skeleton, "foot_l"), at(&skeleton, "foot_r"));
        let hand = at(&skeleton, "hand_l");
        propagate_human_joint_edit(
            &mut skeleton,
            &HumanJointEdit::Move {
                joint: "pelvis".into(),
                x: 0.46,
                y: 0.53,
            },
        )
        .unwrap();
        assert!((at(&skeleton, "pelvis").1 - 0.53).abs() < 0.0001);
        assert!((at(&skeleton, "chest").1 - (0.30 + 0.05)).abs() < 0.0001);
        assert!((at(&skeleton, "hand_l").1 - (hand.1 + 0.05)).abs() < 0.0001);
        assert_eq!(at(&skeleton, "foot_l"), feet.0);
        assert_eq!(at(&skeleton, "foot_r"), feet.1);
        let thigh = human_bone_length("knee_l").unwrap();
        let shin = human_bone_length("ankle_l").unwrap();
        assert!((dist(at(&skeleton, "hip_l"), at(&skeleton, "knee_l")) - thigh).abs() < 0.0001);
        assert!((dist(at(&skeleton, "knee_l"), at(&skeleton, "ankle_l")) - shin).abs() < 0.0001);
        let edit = HumanJointEdit::Move {
            joint: "pelvis".into(),
            x: 0.0,
            y: 0.0,
        };
        let locks = human_joint_locks(&edit);
        assert!(locks.contains(&("foot_l", JointLock::Ground)));
    }

    #[test]
    fn hand_move_locks_the_shoulder_and_restores_canon_length() {
        let mut skeleton = standing();
        let shoulder = at(&skeleton, "shoulder_l");
        let other = at(&skeleton, "hand_r");
        let elbow = skeleton.iter_mut().find(|j| j.id == "elbow_l").unwrap();
        elbow.x -= 0.08;
        let before = skeleton.clone();
        let target = (shoulder.0 - 0.04, shoulder.1 + 0.16);
        propagate_human_joint_edit(
            &mut skeleton,
            &HumanJointEdit::Move {
                joint: "hand_l".into(),
                x: target.0,
                y: target.1,
            },
        )
        .unwrap();
        assert_eq!(at(&skeleton, "shoulder_l"), shoulder);
        assert_eq!(at(&skeleton, "hand_r"), other);
        assert_eq!(at(&skeleton, "pelvis"), at(&before, "pelvis"));
        let upper = human_bone_length("elbow_l").unwrap();
        let fore = human_bone_length("hand_l").unwrap();
        assert!(
            (dist(at(&skeleton, "shoulder_l"), at(&skeleton, "elbow_l")) - upper).abs() < 0.0001
        );
        assert!((dist(at(&skeleton, "elbow_l"), at(&skeleton, "hand_l")) - fore).abs() < 0.0001);
        assert!(at(&skeleton, "elbow_l").0 < at(&skeleton, "shoulder_l").0);
    }

    #[test]
    fn foot_move_locks_the_hip() {
        let mut skeleton = standing();
        let hip = at(&skeleton, "hip_r");
        let other = at(&skeleton, "foot_l");
        let target = (hip.0 + 0.04, hip.1 + 0.20);
        propagate_human_joint_edit(
            &mut skeleton,
            &HumanJointEdit::Move {
                joint: "foot_r".into(),
                x: target.0,
                y: target.1,
            },
        )
        .unwrap();
        assert_eq!(at(&skeleton, "hip_r"), hip);
        assert_eq!(at(&skeleton, "foot_l"), other);
        assert!((at(&skeleton, "foot_r").0 - target.0).abs() < 0.0001);
        let shin = human_bone_length("ankle_r").unwrap();
        assert!((dist(at(&skeleton, "knee_r"), at(&skeleton, "ankle_r")) - shin).abs() < 0.0001);
    }

    #[test]
    fn unreachable_hand_leaves_the_skeleton_unchanged() {
        let mut skeleton = standing();
        let before = skeleton.clone();
        let err = propagate_human_joint_edit(
            &mut skeleton,
            &HumanJointEdit::Move {
                joint: "hand_r".into(),
                x: 0.95,
                y: 0.2,
            },
        )
        .unwrap_err();
        assert!(err.contains("unreachable"));
        assert_eq!(skeleton, before);
    }

    #[test]
    fn head_rotation_does_not_move_joints() {
        let mut skeleton = standing();
        let before: Vec<(String, f32, f32)> =
            skeleton.iter().map(|j| (j.id.clone(), j.x, j.y)).collect();
        propagate_human_joint_edit(
            &mut skeleton,
            &HumanJointEdit::RotateHead {
                pitch: 0.2,
                yaw: -0.4,
                roll: 0.1,
            },
        )
        .unwrap();
        for (id, x, y) in before {
            let joint = skeleton.iter().find(|j| j.id == id).unwrap();
            assert_eq!((joint.x, joint.y), (x, y));
        }
        let head = skeleton.iter().find(|j| j.id == "head").unwrap();
        assert_eq!((head.pitch, head.yaw, head.roll), (0.2, -0.4, 0.1));
        assert_eq!(
            human_segment_rule("head"),
            Some(JointSegmentRule::RotationOnly)
        );
        assert_eq!(human_segment_rule("chest"), Some(JointSegmentRule::Rigid));
        assert_eq!(
            human_segment_rule("knee_l"),
            Some(JointSegmentRule::TwoBone)
        );
    }

    #[test]
    fn reconcile_propagates_one_driver_and_ignores_a_full_pose() {
        let previous = standing();
        let mut one = previous.clone();
        one.iter_mut().find(|j| j.id == "hand_l").unwrap().y = 0.45;
        let solved = reconcile_human_skeleton(&previous, &one)
            .unwrap()
            .expect("one driver");
        assert!((at(&solved, "hand_l").1 - 0.45).abs() < 0.0001);
        assert_eq!(at(&solved, "shoulder_l"), at(&previous, "shoulder_l"));
        let mut many = one;
        many.iter_mut().find(|j| j.id == "pelvis").unwrap().y += 0.04;
        assert!(reconcile_human_skeleton(&previous, &many)
            .unwrap()
            .is_none());
    }
}
