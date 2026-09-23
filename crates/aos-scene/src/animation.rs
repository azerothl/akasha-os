//! Structured rig poses and bounded keyframe playback on SceneGraph joints.

use crate::math::Quat;
use crate::scene::{NodeKind, SceneGraph};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AnimationError {
    #[error("rig root `{0}` has no joints")]
    InvalidRig(String),
    #[error("unknown rig `{0}`")]
    UnknownRig(String),
    #[error("unknown pose `{0}`")]
    UnknownPose(String),
    #[error("animation limit exceeded")]
    Limit,
    #[error("invalid joint rotation")]
    Rotation,
    #[error("no keyframes")]
    EmptyTimeline,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rig {
    pub root_id: String,
    pub joint_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NamedPose {
    pub root_id: String,
    pub name: String,
    pub rotations: BTreeMap<String, Quat>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Keyframe {
    pub root_id: String,
    pub time_ms: u32,
    pub rotations: BTreeMap<String, Quat>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AnimationState {
    #[serde(default)]
    pub rigs: Vec<Rig>,
    #[serde(default)]
    pub poses: Vec<NamedPose>,
    #[serde(default)]
    pub keyframes: Vec<Keyframe>,
    #[serde(default)]
    pub current_ms: u32,
}

impl AnimationState {
    pub fn validate(&self, scene: &SceneGraph) -> Result<(), AnimationError> {
        if self.rigs.len() > 8
            || self.poses.len() > 32
            || self.keyframes.len() > 120
            || self.current_ms > 60_000
        {
            return Err(AnimationError::Limit);
        }
        for rig in &self.rigs {
            if rig.joint_ids.is_empty()
                || rig.joint_ids.len() > 64
                || rig
                    .joint_ids
                    .iter()
                    .any(|id| !is_descendant(scene, id, &rig.root_id))
            {
                return Err(AnimationError::InvalidRig(rig.root_id.clone()));
            }
        }
        for pose in &self.poses {
            self.validate_rotations(&pose.root_id, &pose.rotations)?;
            if pose.name.is_empty() || pose.name.len() > 64 {
                return Err(AnimationError::Limit);
            }
        }
        for frame in &self.keyframes {
            self.validate_rotations(&frame.root_id, &frame.rotations)?;
            if frame.time_ms > 60_000 {
                return Err(AnimationError::Limit);
            }
        }
        Ok(())
    }

    fn rig(&self, root: &str) -> Result<&Rig, AnimationError> {
        self.rigs
            .iter()
            .find(|rig| rig.root_id == root)
            .ok_or_else(|| AnimationError::UnknownRig(root.into()))
    }

    fn validate_rotations(
        &self,
        root: &str,
        rotations: &BTreeMap<String, Quat>,
    ) -> Result<(), AnimationError> {
        let rig = self.rig(root)?;
        if rotations.len() > rig.joint_ids.len()
            || rotations
                .iter()
                .any(|(id, q)| !rig.joint_ids.contains(id) || !q.is_finite() || q.length() < 0.001)
        {
            return Err(AnimationError::Rotation);
        }
        Ok(())
    }

    pub fn register_rig(&mut self, scene: &SceneGraph, root: &str) -> Result<(), AnimationError> {
        if self.rigs.len() >= 8 && self.rigs.iter().all(|rig| rig.root_id != root) {
            return Err(AnimationError::Limit);
        }
        let mut joint_ids: Vec<_> = scene
            .nodes
            .iter()
            .filter(|(id, node)| node.kind == NodeKind::Empty && is_descendant(scene, id, root))
            .map(|(id, _)| id.clone())
            .collect();
        joint_ids.sort();
        joint_ids.truncate(64);
        if joint_ids.is_empty() {
            return Err(AnimationError::InvalidRig(root.into()));
        }
        let rig = Rig {
            root_id: root.into(),
            joint_ids,
        };
        if let Some(existing) = self.rigs.iter_mut().find(|r| r.root_id == root) {
            *existing = rig;
        } else {
            self.rigs.push(rig);
        }
        Ok(())
    }

    fn capture(
        &self,
        scene: &SceneGraph,
        root: &str,
    ) -> Result<BTreeMap<String, Quat>, AnimationError> {
        let rig = self.rig(root)?;
        Ok(rig
            .joint_ids
            .iter()
            .filter_map(|id| {
                scene
                    .nodes
                    .get(id)
                    .map(|node| (id.clone(), node.transform.rotation))
            })
            .collect())
    }

    pub fn save_pose(
        &mut self,
        scene: &SceneGraph,
        root: &str,
        name: &str,
    ) -> Result<(), AnimationError> {
        if name.trim().is_empty() || name.len() > 64 {
            return Err(AnimationError::Limit);
        }
        let pose = NamedPose {
            root_id: root.into(),
            name: name.trim().into(),
            rotations: self.capture(scene, root)?,
        };
        if let Some(existing) = self
            .poses
            .iter_mut()
            .find(|p| p.root_id == root && p.name == pose.name)
        {
            *existing = pose;
        } else if self.poses.len() < 32 {
            self.poses.push(pose);
        } else {
            return Err(AnimationError::Limit);
        }
        Ok(())
    }

    pub fn apply_pose(
        &self,
        scene: &mut SceneGraph,
        root: &str,
        name: &str,
    ) -> Result<(), AnimationError> {
        let pose = self
            .poses
            .iter()
            .find(|p| p.root_id == root && p.name == name)
            .ok_or_else(|| AnimationError::UnknownPose(name.into()))?;
        self.apply_rotations(scene, root, &pose.rotations)
    }

    fn apply_rotations(
        &self,
        scene: &mut SceneGraph,
        root: &str,
        rotations: &BTreeMap<String, Quat>,
    ) -> Result<(), AnimationError> {
        self.validate_rotations(root, rotations)?;
        for (id, rotation) in rotations {
            if let Some(node) = scene.nodes.get_mut(id) {
                node.transform.rotation = rotation.normalized().ok_or(AnimationError::Rotation)?;
            }
        }
        Ok(())
    }

    pub fn add_keyframe(
        &mut self,
        scene: &SceneGraph,
        root: &str,
        time_ms: u32,
    ) -> Result<(), AnimationError> {
        if time_ms > 60_000 {
            return Err(AnimationError::Limit);
        }
        let frame = Keyframe {
            root_id: root.into(),
            time_ms,
            rotations: self.capture(scene, root)?,
        };
        if let Some(existing) = self
            .keyframes
            .iter_mut()
            .find(|f| f.root_id == root && f.time_ms == time_ms)
        {
            *existing = frame;
        } else if self.keyframes.len() < 120 {
            self.keyframes.push(frame);
        } else {
            return Err(AnimationError::Limit);
        }
        self.keyframes
            .sort_by(|a, b| (a.root_id.as_str(), a.time_ms).cmp(&(b.root_id.as_str(), b.time_ms)));
        self.current_ms = time_ms;
        Ok(())
    }

    pub fn duration_ms(&self) -> u32 {
        self.keyframes.iter().map(|f| f.time_ms).max().unwrap_or(0)
    }

    pub fn seek(&mut self, scene: &mut SceneGraph, time_ms: u32) -> Result<(), AnimationError> {
        if self.keyframes.is_empty() {
            return Err(AnimationError::EmptyTimeline);
        }
        let time_ms = time_ms.min(self.duration_ms());
        for rig in &self.rigs {
            let frames: Vec<_> = self
                .keyframes
                .iter()
                .filter(|f| f.root_id == rig.root_id)
                .collect();
            if frames.is_empty() {
                continue;
            }
            let before = frames
                .iter()
                .rev()
                .find(|f| f.time_ms <= time_ms)
                .copied()
                .unwrap_or(frames[0]);
            let after = frames
                .iter()
                .find(|f| f.time_ms >= time_ms)
                .copied()
                .unwrap_or(*frames.last().unwrap());
            let t = if before.time_ms == after.time_ms {
                0.0
            } else {
                (time_ms - before.time_ms) as f32 / (after.time_ms - before.time_ms) as f32
            };
            let mut rotations = BTreeMap::new();
            for id in &rig.joint_ids {
                if let (Some(a), Some(b)) = (before.rotations.get(id), after.rotations.get(id)) {
                    rotations.insert(id.clone(), lerp_quat(*a, *b, t));
                }
            }
            self.apply_rotations(scene, &rig.root_id, &rotations)?;
        }
        self.current_ms = time_ms;
        Ok(())
    }
}

fn is_descendant(scene: &SceneGraph, id: &str, root: &str) -> bool {
    let mut current = scene.nodes.get(id).and_then(|node| node.parent.as_deref());
    for _ in 0..=scene.nodes.len() {
        if current == Some(root) {
            return true;
        }
        current = current
            .and_then(|parent| scene.nodes.get(parent))
            .and_then(|node| node.parent.as_deref());
        if current.is_none() {
            return false;
        }
    }
    false
}

fn lerp_quat(a: Quat, b: Quat, t: f32) -> Quat {
    let dot = a.x * b.x + a.y * b.y + a.z * b.z + a.w * b.w;
    let b = if dot < 0.0 {
        Quat::from_xyzw(-b.x, -b.y, -b.z, -b.w)
    } else {
        b
    };
    Quat::from_xyzw(
        a.x + (b.x - a.x) * t,
        a.y + (b.y - a.y) * t,
        a.z + (b.z - a.z) * t,
        a.w + (b.w - a.w) * t,
    )
    .normalized()
    .unwrap_or(Quat::IDENTITY)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::Vec3;

    #[test]
    fn rig_pose_and_keyframes_interpolate_and_validate() {
        let mut scene = SceneGraph::demo_scene();
        let mut animation = AnimationState::default();
        animation.register_rig(&scene, "humanoid").unwrap();
        assert!(animation.rigs[0].joint_ids.contains(&"upper_arm_r".into()));
        animation.save_pose(&scene, "humanoid", "rest").unwrap();
        animation.add_keyframe(&scene, "humanoid", 0).unwrap();
        scene
            .nodes
            .get_mut("upper_arm_r")
            .unwrap()
            .transform
            .rotation = Quat::from_axis_angle(Vec3::UNIT_Z, 1.0);
        animation.save_pose(&scene, "humanoid", "wave").unwrap();
        animation.add_keyframe(&scene, "humanoid", 1000).unwrap();
        animation.seek(&mut scene, 500).unwrap();
        let middle = scene.nodes["upper_arm_r"].transform.rotation;
        assert!(middle.z > 0.1 && middle.z < 0.5);
        animation
            .apply_pose(&mut scene, "humanoid", "rest")
            .unwrap();
        assert_eq!(
            scene.nodes["upper_arm_r"].transform.rotation,
            Quat::IDENTITY
        );
        assert!(animation.validate(&scene).is_ok());
    }
}
