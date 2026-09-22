//! Storyboard sequencing — ordered shot frames over a shared SceneGraph.
//!
//! Spec §161: each shot stores camera, character poses, visibility (and light
//! node transforms as lighting overrides). SceneGraph remains the only SoT for
//! the active edit/beauty view; the timeline is differential overrides.

use crate::scene::{CameraParams, SceneError, SceneGraph, Transform};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

/// DeclUI / host service: capture the current SceneGraph as a new timeline frame.
pub const STORYBOARD_CAPTURE_SERVICE: &str = "storyboard.capture";
/// DeclUI / host service: apply / step the active storyboard frame onto the scene.
pub const STORYBOARD_APPLY_SERVICE: &str = "storyboard.apply";
/// DeclUI / host service: delete a storyboard frame.
pub const STORYBOARD_DELETE_SERVICE: &str = "storyboard.delete";
/// DeclUI / host service: reorder the active frame earlier / later on the timeline.
pub const STORYBOARD_MOVE_SERVICE: &str = "storyboard.move";

/// Cap: mutate the illustration storyboard timeline (fail-closed).
pub const STORYBOARD_EDIT_CAP: &str = "storyboard.edit";

/// On-disk storyboard format version (nested under ProjectFile).
pub const STORYBOARD_FORMAT_VERSION: u32 = 1;

/// Default frame duration for future animatics (ms).
pub const DEFAULT_FRAME_DURATION_MS: u32 = 1000;

fn default_duration() -> u32 {
    DEFAULT_FRAME_DURATION_MS
}

#[derive(Debug, Error, PartialEq)]
pub enum StoryboardError {
    #[error("scene: {0}")]
    Scene(String),
    #[error("empty storyboard")]
    Empty,
    #[error("unknown frame `{0}`")]
    UnknownFrame(String),
    #[error("frame index {0} out of range (len {1})")]
    IndexOutOfRange(usize, usize),
    #[error("unsupported storyboard format_version {0}")]
    UnsupportedVersion(u32),
    #[error("invalid move direction `{0}` (use earlier|later)")]
    InvalidDirection(String),
}

impl From<SceneError> for StoryboardError {
    fn from(e: SceneError) -> Self {
        Self::Scene(e.to_string())
    }
}

/// Ordered shot timeline sharing one base SceneGraph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Storyboard {
    #[serde(default = "storyboard_format_default")]
    pub format_version: u32,
    #[serde(default)]
    pub frames: Vec<StoryFrame>,
    /// Index of the active frame (clamped on apply).
    #[serde(default)]
    pub active_index: usize,
}

impl Default for Storyboard {
    fn default() -> Self {
        Self {
            format_version: STORYBOARD_FORMAT_VERSION,
            frames: Vec::new(),
            active_index: 0,
        }
    }
}

fn storyboard_format_default() -> u32 {
    STORYBOARD_FORMAT_VERSION
}

/// One timeline shot: camera + pose/visibility overrides over shared assets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoryFrame {
    pub id: String,
    pub label: String,
    #[serde(default = "default_duration")]
    pub duration_ms: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_camera: Option<String>,
    /// Node id → local TRS (character poses, camera placement, light placement).
    #[serde(default)]
    pub transforms: BTreeMap<String, Transform>,
    /// Node id → visibility.
    #[serde(default)]
    pub visibility: BTreeMap<String, bool>,
    /// Camera node id → lens params (focal / sensor / clip).
    #[serde(default)]
    pub cameras: BTreeMap<String, CameraParams>,
}

impl Storyboard {
    pub fn validate(&self) -> Result<(), StoryboardError> {
        if self.format_version != STORYBOARD_FORMAT_VERSION {
            return Err(StoryboardError::UnsupportedVersion(self.format_version));
        }
        let mut seen = std::collections::HashSet::new();
        for frame in &self.frames {
            if !seen.insert(frame.id.as_str()) {
                return Err(StoryboardError::UnknownFrame(format!(
                    "duplicate frame id `{}`",
                    frame.id
                )));
            }
            for t in frame.transforms.values() {
                t.validate()?;
            }
        }
        if !self.frames.is_empty() && self.active_index >= self.frames.len() {
            return Err(StoryboardError::IndexOutOfRange(
                self.active_index,
                self.frames.len(),
            ));
        }
        Ok(())
    }

    pub fn summary(&self) -> String {
        if self.frames.is_empty() {
            return "0 frames".into();
        }
        let idx = self.active_index.min(self.frames.len() - 1);
        let label = &self.frames[idx].label;
        format!("Frame {}/{} — {}", idx + 1, self.frames.len(), label)
    }

    pub fn active_frame(&self) -> Option<&StoryFrame> {
        self.frames.get(self.active_index)
    }

    fn next_frame_id(&self) -> String {
        let n = self.frames.len() + 1;
        let candidate = format!("shot_{n:02}");
        if self.frames.iter().all(|f| f.id != candidate) {
            return candidate;
        }
        for i in 1..10_000 {
            let id = format!("shot_{i:04}");
            if self.frames.iter().all(|f| f.id != id) {
                return id;
            }
        }
        format!("shot_{}", uuid_like())
    }
}

fn uuid_like() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1)
}

/// Capture the current SceneGraph into a new storyboard frame (appended, becomes active).
pub fn capture_frame(
    board: &mut Storyboard,
    scene: &SceneGraph,
    label: Option<&str>,
    duration_ms: Option<u32>,
) -> Result<StoryFrame, StoryboardError> {
    board.validate()?;
    let id = board.next_frame_id();
    let index = board.frames.len() + 1;
    let label = label
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("Shot {index:02}"));
    let mut transforms = BTreeMap::new();
    let mut visibility = BTreeMap::new();
    let mut cameras = BTreeMap::new();
    for (nid, node) in &scene.nodes {
        transforms.insert(nid.clone(), node.transform.clone());
        visibility.insert(nid.clone(), node.visible);
        if let Some(cam) = &node.camera {
            cameras.insert(nid.clone(), cam.clone());
        }
    }
    let frame = StoryFrame {
        id,
        label,
        duration_ms: duration_ms.unwrap_or(DEFAULT_FRAME_DURATION_MS).max(1),
        active_camera: scene.active_camera.clone(),
        transforms,
        visibility,
        cameras,
    };
    board.frames.push(frame.clone());
    board.active_index = board.frames.len() - 1;
    board.format_version = STORYBOARD_FORMAT_VERSION;
    board.validate()?;
    Ok(frame)
}

/// Apply a frame's overrides onto `scene`. Updates `board.active_index`.
pub fn apply_frame<'a>(
    board: &'a mut Storyboard,
    scene: &mut SceneGraph,
    target: ApplyTarget,
) -> Result<&'a StoryFrame, StoryboardError> {
    board.validate()?;
    if board.frames.is_empty() {
        return Err(StoryboardError::Empty);
    }
    let index = match target {
        ApplyTarget::Index(i) => {
            if i >= board.frames.len() {
                return Err(StoryboardError::IndexOutOfRange(i, board.frames.len()));
            }
            i
        }
        ApplyTarget::Id(id) => board
            .frames
            .iter()
            .position(|f| f.id == id)
            .ok_or(StoryboardError::UnknownFrame(id))?,
        ApplyTarget::Step(delta) => {
            let cur = board.active_index.min(board.frames.len() - 1);
            let next = cur as isize + delta;
            if next < 0 || next as usize >= board.frames.len() {
                return Err(StoryboardError::IndexOutOfRange(
                    next.max(0) as usize,
                    board.frames.len(),
                ));
            }
            next as usize
        }
        ApplyTarget::Active => board.active_index.min(board.frames.len() - 1),
    };

    let frame = &board.frames[index];
    for (nid, t) in &frame.transforms {
        if scene.nodes.contains_key(nid) {
            scene.set_transform(nid, t.clone())?;
        }
    }
    for (nid, vis) in &frame.visibility {
        if let Some(node) = scene.nodes.get_mut(nid) {
            node.visible = *vis;
        }
    }
    for (nid, cam) in &frame.cameras {
        if let Some(node) = scene.nodes.get_mut(nid) {
            node.camera = Some(cam.clone());
        }
    }
    if let Some(cam_id) = &frame.active_camera {
        if scene.nodes.contains_key(cam_id) {
            scene.active_camera = Some(cam_id.clone());
        }
    }
    board.active_index = index;
    scene.validate()?;
    Ok(&board.frames[index])
}

#[derive(Debug, Clone, PartialEq)]
pub enum ApplyTarget {
    Active,
    Index(usize),
    Id(String),
    Step(isize),
}

/// Delete a frame by id or the active frame when `id` is None.
pub fn delete_frame(
    board: &mut Storyboard,
    id: Option<&str>,
) -> Result<StoryFrame, StoryboardError> {
    board.validate()?;
    if board.frames.is_empty() {
        return Err(StoryboardError::Empty);
    }
    let index = if let Some(id) = id {
        board
            .frames
            .iter()
            .position(|f| f.id == id)
            .ok_or_else(|| StoryboardError::UnknownFrame(id.into()))?
    } else {
        board.active_index.min(board.frames.len() - 1)
    };
    let removed = board.frames.remove(index);
    if board.frames.is_empty() {
        board.active_index = 0;
    } else if board.active_index >= board.frames.len() {
        board.active_index = board.frames.len() - 1;
    } else if index < board.active_index {
        board.active_index -= 1;
    }
    Ok(removed)
}

/// Move the active frame earlier (−1) or later (+1) on the timeline.
pub fn move_active_frame(board: &mut Storyboard, direction: &str) -> Result<usize, StoryboardError> {
    board.validate()?;
    if board.frames.is_empty() {
        return Err(StoryboardError::Empty);
    }
    let delta: isize = match direction.trim().to_ascii_lowercase().as_str() {
        "earlier" | "up" | "left" | "prev" | "-1" => -1,
        "later" | "down" | "right" | "next" | "+1" => 1,
        other => return Err(StoryboardError::InvalidDirection(other.into())),
    };
    let cur = board.active_index.min(board.frames.len() - 1);
    let next = cur as isize + delta;
    if next < 0 || next as usize >= board.frames.len() {
        return Err(StoryboardError::IndexOutOfRange(
            next.max(0) as usize,
            board.frames.len(),
        ));
    }
    let next = next as usize;
    board.frames.swap(cur, next);
    board.active_index = next;
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::Vec3;
    use crate::ops::UndoStack;
    use crate::pose::apply_pose_preset;

    #[test]
    fn capture_apply_round_trip_preserves_pose() {
        use crate::pose::{joint_node_id, JointId};

        let mut scene = SceneGraph::demo_scene();
        let mut undo = UndoStack::default();
        apply_pose_preset(&mut scene, "humanoid", "wave_right", Some(&mut undo)).unwrap();
        let arm_id = joint_node_id(&scene, "humanoid", JointId::UpperArmR).expect("upper arm");
        let waved_arm = scene.nodes[&arm_id].transform.clone();
        assert_ne!(
            waved_arm.rotation,
            crate::math::Quat::IDENTITY,
            "wave_right should rotate upper_arm_r"
        );

        let mut board = Storyboard::default();
        let frame = capture_frame(&mut board, &scene, Some("Wave"), None).unwrap();
        assert_eq!(frame.label, "Wave");
        assert_eq!(board.frames.len(), 1);

        // Mutate the scene away from the captured pose (direct transform, not rest —
        // articulated reset sets can be sparse vs legacy flat joints).
        let mut away = waved_arm.clone();
        away.rotation = crate::math::Quat::IDENTITY;
        scene.set_transform(&arm_id, away).unwrap();
        assert_ne!(scene.nodes[&arm_id].transform, waved_arm);

        apply_frame(&mut board, &mut scene, ApplyTarget::Active).unwrap();
        assert_eq!(scene.nodes[&arm_id].transform, waved_arm);
        assert_eq!(board.summary(), "Frame 1/1 — Wave");
    }

    #[test]
    fn step_between_frames() {
        let mut scene = SceneGraph::demo_scene();
        let mut board = Storyboard::default();
        capture_frame(&mut board, &scene, Some("Wide"), None).unwrap();

        scene.nodes.get_mut("camera").unwrap().transform.translation = Vec3::new(0.0, 1.5, 3.0);
        capture_frame(&mut board, &scene, Some("Close-up"), None).unwrap();
        assert_eq!(board.active_index, 1);

        apply_frame(&mut board, &mut scene, ApplyTarget::Step(-1)).unwrap();
        assert_eq!(board.active_index, 0);
        assert!((scene.nodes["camera"].transform.translation.z - 6.5).abs() < 1e-4);

        apply_frame(&mut board, &mut scene, ApplyTarget::Step(1)).unwrap();
        assert_eq!(board.active_index, 1);
        assert!((scene.nodes["camera"].transform.translation.z - 3.0).abs() < 1e-4);
    }

    #[test]
    fn delete_and_reorder() {
        let scene = SceneGraph::demo_scene();
        let mut board = Storyboard::default();
        capture_frame(&mut board, &scene, Some("A"), None).unwrap();
        capture_frame(&mut board, &scene, Some("B"), None).unwrap();
        capture_frame(&mut board, &scene, Some("C"), None).unwrap();
        assert_eq!(board.active_index, 2);

        move_active_frame(&mut board, "earlier").unwrap();
        assert_eq!(board.frames[1].label, "C");
        assert_eq!(board.frames[2].label, "B");
        assert_eq!(board.active_index, 1);

        delete_frame(&mut board, None).unwrap();
        assert_eq!(board.frames.len(), 2);
        assert_eq!(board.frames[0].label, "A");
        assert_eq!(board.frames[1].label, "B");
    }

    #[test]
    fn empty_apply_fails_closed() {
        let mut board = Storyboard::default();
        let mut scene = SceneGraph::demo_scene();
        let err = apply_frame(&mut board, &mut scene, ApplyTarget::Active).unwrap_err();
        assert_eq!(err, StoryboardError::Empty);
    }
}
