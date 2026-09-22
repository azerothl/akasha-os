//! Transactional agent edit batches over a SceneGraph project.
//!
//! Spec §132: complex agent edits apply as one transaction — validate then
//! commit, otherwise rollback so the scene never stays half-modified.
//!
//! SceneGraph remains the only SoT; this module never touches the host FS.

use crate::assets::{embedded_primitives_pack, instantiate_asset, AssetError};
use crate::camera::{
    camera_transform_look_at, eye_from_orbit, orbit_from_active_camera, set_focal_from_hfov,
};
use crate::compose::{compose_from_prompt, ComposeError};
use crate::light::{color_from_srgb_u8, merge_light_params, parse_light_type};
use crate::locks::{
    LockError, LockKind, LockScope, LockTable, MutateKind, SemanticLock, SCENE_LOCK_CAP,
};
use crate::math::{Quat, Vec3};
use crate::ops::{SceneOp, UndoStack};
use crate::pose::{apply_pose, apply_pose_preset, JointId, PoseError, PoseOp};
use crate::project::{load_project_yaml, save_project_yaml, ProjectError, ProjectFile};
use crate::scene::{LightParams, NodeKind, SceneError, SceneGraph, Transform};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Cap: select / TRS / transactional apply (fail-closed).
pub const SCENE_EDIT_CAP: &str = "scene.edit";

/// DeclUI / host_call: read project snapshot (yaml + selection + locks).
pub const SCENE_GET_SERVICE: &str = "scene.get";

/// DeclUI / host_call: set selection (non-mutating for locks).
pub const SCENE_SELECT_SERVICE: &str = "scene.select";

/// DeclUI / host_call: set node TRS.
pub const SCENE_TRS_SERVICE: &str = "scene.trs";

/// DeclUI / host_call: set active camera orbit / look-at / FOV.
pub const SCENE_CAMERA_SERVICE: &str = "scene.camera";

/// DeclUI / host_call: add / edit SceneGraph Light nodes.
pub const SCENE_LIGHT_SERVICE: &str = "scene.light";

/// DeclUI / host_call: transactional batch apply / rollback.
pub const SCENE_APPLY_SERVICE: &str = "scene.apply";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum AgentEditOp {
    Select {
        id: String,
    },
    Trs {
        id: String,
        #[serde(default)]
        translation: Option<Vec3>,
        #[serde(default)]
        rotation: Option<Quat>,
        #[serde(default)]
        scale: Option<Vec3>,
    },
    /// Active (or named) camera: eye / look-at / horizontal FOV degrees.
    Camera {
        #[serde(default)]
        id: Option<String>,
        #[serde(default)]
        eye: Option<Vec3>,
        #[serde(default)]
        look_at: Option<Vec3>,
        /// Horizontal FOV in **degrees** (UI / agent boundary).
        #[serde(default)]
        fov_deg: Option<f32>,
        #[serde(default)]
        yaw: Option<f32>,
        #[serde(default)]
        pitch: Option<f32>,
        #[serde(default)]
        distance: Option<f32>,
    },
    /// Add a Light node and/or edit intensity / color / type / translation.
    Light {
        #[serde(default)]
        id: Option<String>,
        /// When true, insert a new Light (id optional; auto `light_N`).
        #[serde(default)]
        add: bool,
        #[serde(default)]
        parent_id: Option<String>,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        translation: Option<Vec3>,
        /// `point` | `directional` | `spot` (EN; FR aliases accepted in parse).
        #[serde(default)]
        light_type: Option<String>,
        #[serde(default)]
        intensity: Option<f32>,
        /// Linear RGB when set; prefer `color_srgb` from DeclUI.
        #[serde(default)]
        color: Option<[f32; 3]>,
        /// sRGB 0–255 channels from DeclUI (converted to linear at boundary).
        #[serde(default)]
        color_srgb: Option<[u8; 3]>,
        #[serde(default)]
        range: Option<f32>,
        /// Spot outer half-angle in **degrees** (UI boundary).
        #[serde(default)]
        spot_angle_deg: Option<f32>,
    },
    Pose {
        humanoid_root: String,
        #[serde(default)]
        preset: Option<String>,
        #[serde(default)]
        look_at: Option<Vec3>,
        #[serde(default)]
        joint: Option<String>,
        #[serde(default)]
        axis: Option<Vec3>,
        #[serde(default)]
        angle_rad: Option<f32>,
    },
    Compose {
        prompt: String,
    },
    Instantiate {
        asset_id: String,
        #[serde(default)]
        parent_id: Option<String>,
        #[serde(default)]
        prefix: Option<String>,
    },
    Lock {
        id: String,
        #[serde(default)]
        scope: LockScope,
        #[serde(default)]
        kind: LockKind,
    },
    Unlock {
        id: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditSnapshot {
    pub project: ProjectFile,
    pub selected_id: Option<String>,
}

impl EditSnapshot {
    pub fn from_yaml(yaml: &str) -> Result<Self, EditError> {
        let project = if yaml.trim().is_empty() {
            ProjectFile::new(SceneGraph::demo_scene())
        } else {
            load_project_yaml(yaml)?
        };
        let selected_id = project.selected_id.clone();
        Ok(Self {
            project,
            selected_id,
        })
    }

    pub fn to_yaml(&self) -> Result<String, EditError> {
        let mut p = self.project.clone();
        p.selected_id = self.selected_id.clone();
        Ok(save_project_yaml(&p)?)
    }

    pub fn result_json(&self) -> Result<serde_json::Value, EditError> {
        let yaml = self.to_yaml()?;
        Ok(serde_json::json!({
            "scene_yaml": yaml,
            "selected_id": self.selected_id,
            "locks": self.project.locks.list().iter().map(|l| {
                serde_json::json!({
                    "node_id": l.node_id,
                    "scope": l.scope,
                    "kind": l.kind,
                    "holder": l.holder,
                })
            }).collect::<Vec<_>>(),
            "root_id": self.selected_id,
        }))
    }
}

#[derive(Debug, Error)]
pub enum EditError {
    #[error("project: {0}")]
    Project(#[from] ProjectError),
    #[error("scene: {0}")]
    Scene(#[from] SceneError),
    #[error("lock: {0}")]
    Lock(#[from] LockError),
    #[error("pose: {0}")]
    Pose(#[from] PoseError),
    #[error("compose: {0}")]
    Compose(#[from] ComposeError),
    #[error("asset: {0}")]
    Asset(#[from] AssetError),
    #[error("unknown node `{0}`")]
    UnknownNode(String),
    #[error("pose op requires preset, look_at, or joint+axis+angle_rad")]
    IncompletePose,
    #[error("camera op requires eye+look_at, orbit (yaw/pitch/distance+look_at), or fov_deg")]
    IncompleteCamera,
    #[error("light op requires add and/or id with intensity/color/type/translation")]
    IncompleteLight,
    #[error("unknown light type `{0}`")]
    UnknownLightType(String),
    #[error("empty apply batch")]
    EmptyBatch,
    #[error("actor `{0}` denied — missing capability")]
    CapDenied(String),
}

/// Who is applying the edit — agents respect semantic locks; humans may override.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditActorKind {
    Human,
    Agent,
}

impl EditActorKind {
    pub fn from_actor(actor: &str) -> Self {
        if actor.starts_with("agent:") {
            Self::Agent
        } else {
            Self::Human
        }
    }
}

/// Apply a single op in-place. Used by transactional `apply_batch`.
pub fn apply_one(
    snap: &mut EditSnapshot,
    op: &AgentEditOp,
    actor: &str,
    actor_kind: EditActorKind,
) -> Result<(), EditError> {
    match op {
        AgentEditOp::Select { id } => {
            if !snap.project.scene.nodes.contains_key(id) {
                return Err(EditError::UnknownNode(id.clone()));
            }
            snap.selected_id = Some(id.clone());
            Ok(())
        }
        AgentEditOp::Trs {
            id,
            translation,
            rotation,
            scale,
        } => {
            if actor_kind == EditActorKind::Agent {
                snap.project.locks.assert_agent_may_mutate(
                    &snap.project.scene,
                    id,
                    MutateKind::Transform,
                )?;
            }
            let before = snap
                .project
                .scene
                .nodes
                .get(id)
                .ok_or_else(|| EditError::UnknownNode(id.clone()))?
                .transform
                .clone();
            let mut after = before.clone();
            if let Some(t) = translation {
                after.translation = *t;
            }
            if let Some(r) = rotation {
                after.rotation = *r;
            }
            if let Some(s) = scale {
                after.scale = *s;
            }
            let op = SceneOp::SetTransform {
                id: id.clone(),
                before,
                after,
            };
            let mut undo = UndoStack::default();
            undo.push_apply(&mut snap.project.scene, op)?;
            snap.selected_id = Some(id.clone());
            Ok(())
        }
        AgentEditOp::Camera {
            id,
            eye,
            look_at,
            fov_deg,
            yaw,
            pitch,
            distance,
        } => {
            let cam_id = id
                .clone()
                .or_else(|| snap.project.scene.active_camera.clone())
                .ok_or_else(|| EditError::UnknownNode("camera".into()))?;
            if actor_kind == EditActorKind::Agent {
                snap.project.locks.assert_agent_may_mutate(
                    &snap.project.scene,
                    &cam_id,
                    MutateKind::Transform,
                )?;
            }
            let node = snap
                .project
                .scene
                .nodes
                .get(&cam_id)
                .ok_or_else(|| EditError::UnknownNode(cam_id.clone()))?
                .clone();
            if node.kind != NodeKind::Camera {
                return Err(EditError::UnknownNode(cam_id));
            }
            let before = node.transform.clone();
            let before_params = node.camera.clone().unwrap_or_default();
            let mut after_params = before_params.clone();
            if let Some(d) = fov_deg {
                set_focal_from_hfov(&mut after_params, d.to_radians());
            }

            let after = match (eye, look_at, yaw, pitch, distance) {
                (Some(e), Some(t), _, _, _) => camera_transform_look_at(*e, *t),
                (_, Some(t), Some(y), Some(p), Some(d)) => {
                    let e = eye_from_orbit(*y, *p, *d, *t);
                    camera_transform_look_at(e, *t)
                }
                (_, None, Some(y), Some(p), Some(d)) => {
                    let target = orbit_from_active_camera(&snap.project.scene, *d)
                        .map(|(_, _, _, t, _, _)| t)
                        .unwrap_or(Vec3::new(0.4, 0.8, 0.0));
                    let e = eye_from_orbit(*y, *p, *d, target);
                    camera_transform_look_at(e, target)
                }
                (None, None, None, None, None) if fov_deg.is_some() => before.clone(),
                _ => return Err(EditError::IncompleteCamera),
            };

            let op = SceneOp::SetCamera {
                id: cam_id.clone(),
                before,
                after,
                before_params,
                after_params,
            };
            let mut undo = UndoStack::default();
            undo.push_apply(&mut snap.project.scene, op)?;
            snap.project.scene.active_camera = Some(cam_id.clone());
            snap.selected_id = Some(cam_id);
            Ok(())
        }
        AgentEditOp::Light {
            id,
            add,
            parent_id,
            name,
            translation,
            light_type,
            intensity,
            color,
            color_srgb,
            range,
            spot_angle_deg,
        } => {
            let parsed_type = match light_type {
                Some(s) => Some(parse_light_type(s).ok_or_else(|| {
                    EditError::UnknownLightType(s.clone())
                })?),
                None => None,
            };
            let linear_color = color.or_else(|| color_srgb.map(color_from_srgb_u8));
            let spot_rad = spot_angle_deg.map(|d| d.to_radians());
            let has_patch = translation.is_some()
                || parsed_type.is_some()
                || intensity.is_some()
                || linear_color.is_some()
                || range.is_some()
                || spot_rad.is_some();

            if *add {
                let parent = parent_id.as_deref().unwrap_or("root");
                if actor_kind == EditActorKind::Agent {
                    snap.project.locks.assert_agent_may_mutate(
                        &snap.project.scene,
                        parent,
                        MutateKind::Structure,
                    )?;
                }
                let new_id = id.clone().unwrap_or_else(|| next_light_id(&snap.project.scene));
                let mut params = LightParams::default();
                if let Some(t) = parsed_type {
                    params.light_type = t;
                }
                if let Some(i) = intensity {
                    params.intensity = *i;
                }
                if let Some(c) = linear_color {
                    params.color = c;
                }
                if let Some(r) = range {
                    params.range = *r;
                }
                if let Some(a) = spot_rad {
                    params.spot_angle_rad = a;
                }
                let transform = Transform {
                    translation: translation.unwrap_or(Vec3::new(2.0, 3.5, 2.0)),
                    rotation: Quat::IDENTITY,
                    scale: Vec3::ONE,
                };
                let op = SceneOp::InsertLight {
                    id: new_id.clone(),
                    name: name.clone().unwrap_or_else(|| "Light".into()),
                    parent: parent.into(),
                    transform,
                    params,
                };
                let mut undo = UndoStack::default();
                undo.push_apply(&mut snap.project.scene, op)?;
                snap.selected_id = Some(new_id);
                return Ok(());
            }

            let light_id = id
                .clone()
                .or_else(|| snap.selected_id.clone())
                .ok_or(EditError::IncompleteLight)?;
            if !has_patch {
                return Err(EditError::IncompleteLight);
            }
            if actor_kind == EditActorKind::Agent {
                snap.project.locks.assert_agent_may_mutate(
                    &snap.project.scene,
                    &light_id,
                    MutateKind::Transform,
                )?;
            }
            let node = snap
                .project
                .scene
                .nodes
                .get(&light_id)
                .ok_or_else(|| EditError::UnknownNode(light_id.clone()))?
                .clone();
            if node.kind != NodeKind::Light {
                return Err(EditError::UnknownNode(light_id));
            }
            let before = node.transform.clone();
            let before_params = node.light.clone().unwrap_or_default();
            let after_params = merge_light_params(
                &before_params,
                parsed_type,
                *intensity,
                linear_color,
                *range,
                spot_rad,
            );
            let mut after = before.clone();
            if let Some(t) = translation {
                after.translation = *t;
            }
            let op = SceneOp::SetLight {
                id: light_id.clone(),
                before,
                after,
                before_params,
                after_params,
            };
            let mut undo = UndoStack::default();
            undo.push_apply(&mut snap.project.scene, op)?;
            snap.selected_id = Some(light_id);
            Ok(())
        }
        AgentEditOp::Pose {
            humanoid_root,
            preset,
            look_at,
            joint,
            axis,
            angle_rad,
        } => {
            if actor_kind == EditActorKind::Agent {
                snap.project.locks.assert_agent_may_mutate(
                    &snap.project.scene,
                    humanoid_root,
                    MutateKind::Pose,
                )?;
            }
            let mut undo = UndoStack::default();
            if let Some(p) = preset {
                apply_pose_preset(
                    &mut snap.project.scene,
                    humanoid_root,
                    p,
                    Some(&mut undo),
                )?;
            } else if let Some(target) = look_at {
                apply_pose(
                    &mut snap.project.scene,
                    &PoseOp::LookAt {
                        character_root: humanoid_root.clone(),
                        target_world: *target,
                    },
                    Some(&mut undo),
                )?;
            } else if let (Some(j), Some(ax), Some(ang)) = (joint, axis, angle_rad) {
                let joint = JointId::parse(j)
                    .ok_or_else(|| PoseError::UnknownJoint(j.clone(), humanoid_root.clone()))?;
                apply_pose(
                    &mut snap.project.scene,
                    &PoseOp::RotateJoint {
                        character_root: humanoid_root.clone(),
                        joint,
                        axis: *ax,
                        angle_rad: *ang,
                    },
                    Some(&mut undo),
                )?;
            } else {
                return Err(EditError::IncompletePose);
            }
            snap.selected_id = Some(humanoid_root.clone());
            Ok(())
        }
        AgentEditOp::Compose { prompt } => {
            if actor_kind == EditActorKind::Agent && !snap.project.locks.is_empty() {
                // Fail-closed: replace-all compose would discard user locks.
                if let Some(lock) = snap.project.locks.list().into_iter().next() {
                    return Err(EditError::Lock(LockError::Blocked {
                        node_id: "*".into(),
                        lock_node: lock.node_id,
                        kind: lock.kind,
                    }));
                }
            }
            let composed = compose_from_prompt(prompt)?;
            snap.project.scene = composed.scene;
            snap.project.locks = LockTable::default();
            snap.selected_id = composed
                .character_id
                .or(Some(composed.camera_id));
            Ok(())
        }
        AgentEditOp::Instantiate {
            asset_id,
            parent_id,
            prefix,
        } => {
            let parent = parent_id.as_deref().unwrap_or("root");
            if actor_kind == EditActorKind::Agent {
                snap.project.locks.assert_agent_may_mutate(
                    &snap.project.scene,
                    parent,
                    MutateKind::Structure,
                )?;
            }
            let pack = embedded_primitives_pack()?;
            let prefix = prefix.as_deref().unwrap_or("inst_");
            let inst = instantiate_asset(
                &mut snap.project.scene,
                &pack,
                asset_id,
                Some(parent),
                prefix,
            )?;
            snap.selected_id = Some(inst.root_id);
            Ok(())
        }
        AgentEditOp::Lock { id, scope, kind } => {
            if !snap.project.scene.nodes.contains_key(id) {
                return Err(EditError::UnknownNode(id.clone()));
            }
            snap.project.locks.set(SemanticLock {
                node_id: id.clone(),
                scope: *scope,
                kind: *kind,
                holder: actor.to_string(),
            });
            Ok(())
        }
        AgentEditOp::Unlock { id } => {
            snap.project.locks.remove(id);
            Ok(())
        }
    }
}

/// Apply all ops atomically. On any error the snapshot is left unchanged
/// (caller keeps the pre-batch copy).
pub fn apply_batch(
    snap: &mut EditSnapshot,
    ops: &[AgentEditOp],
    actor: &str,
    actor_kind: EditActorKind,
) -> Result<(), EditError> {
    if ops.is_empty() {
        return Err(EditError::EmptyBatch);
    }
    let backup = snap.clone();
    for op in ops {
        if let Err(e) = apply_one(snap, op, actor, actor_kind) {
            *snap = backup;
            return Err(e);
        }
    }
    snap.project.scene.validate()?;
    Ok(())
}

/// Capability gate helper for host / DeclUI (fail-closed).
pub fn require_edit_caps(op: &AgentEditOp, granted: &[String]) -> Result<(), EditError> {
    let need = match op {
        AgentEditOp::Select { .. }
        | AgentEditOp::Trs { .. }
        | AgentEditOp::Camera { .. }
        | AgentEditOp::Light { .. }
        | AgentEditOp::Instantiate { .. } => SCENE_EDIT_CAP,
        AgentEditOp::Pose { .. } => crate::pose::SCENE_POSE_CAP,
        AgentEditOp::Compose { .. } => crate::compose::SCENE_COMPOSE_CAP,
        AgentEditOp::Lock { .. } | AgentEditOp::Unlock { .. } => SCENE_LOCK_CAP,
    };
    if granted.iter().any(|c| c == need || c == "tool.invoke:*") {
        // Compose also needs asset read — checked by DeclUI validator.
        if matches!(op, AgentEditOp::Compose { .. })
            && !granted
                .iter()
                .any(|c| c == crate::assets::ASSET_ILLUSTRATION_READ_CAP)
        {
            return Err(EditError::CapDenied(
                crate::assets::ASSET_ILLUSTRATION_READ_CAP.into(),
            ));
        }
        if matches!(op, AgentEditOp::Instantiate { .. })
            && !granted
                .iter()
                .any(|c| c == crate::assets::ASSET_ILLUSTRATION_READ_CAP)
        {
            return Err(EditError::CapDenied(
                crate::assets::ASSET_ILLUSTRATION_READ_CAP.into(),
            ));
        }
        return Ok(());
    }
    Err(EditError::CapDenied(need.into()))
}

pub fn require_batch_caps(ops: &[AgentEditOp], granted: &[String]) -> Result<(), EditError> {
    for op in ops {
        require_edit_caps(op, granted)?;
    }
    Ok(())
}

/// Build a `Transform` patch helper for DeclUI TRS input.
pub fn merge_trs(
    before: &Transform,
    translation: Option<Vec3>,
    rotation: Option<Quat>,
    scale: Option<Vec3>,
) -> Transform {
    let mut after = before.clone();
    if let Some(t) = translation {
        after.translation = t;
    }
    if let Some(r) = rotation {
        after.rotation = r;
    }
    if let Some(s) = scale {
        after.scale = s;
    }
    after
}

fn next_light_id(scene: &SceneGraph) -> String {
    for i in 1..10_000 {
        let id = format!("light_{i}");
        if !scene.nodes.contains_key(&id) {
            return id;
        }
    }
    format!("light_{}", scene.nodes.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::Vec3;

    fn demo_snap() -> EditSnapshot {
        EditSnapshot {
            project: ProjectFile::new(SceneGraph::demo_scene()),
            selected_id: Some("box".into()),
        }
    }

    #[test]
    fn batch_rolls_back_on_lock() {
        let mut snap = demo_snap();
        snap.project.locks.set(SemanticLock {
            node_id: "box".into(),
            scope: LockScope::Node,
            kind: LockKind::Semantic,
            holder: "user".into(),
        });
        let before = snap.project.scene.nodes["box"].transform.translation;
        let err = apply_batch(
            &mut snap,
            &[
                AgentEditOp::Select {
                    id: "humanoid".into(),
                },
                AgentEditOp::Trs {
                    id: "box".into(),
                    translation: Some(Vec3::new(9.0, 0.0, 0.0)),
                    rotation: None,
                    scale: None,
                },
            ],
            "agent:test",
            EditActorKind::Agent,
        )
        .unwrap_err();
        assert!(matches!(err, EditError::Lock(_)));
        assert_eq!(
            snap.project.scene.nodes["box"].transform.translation,
            before
        );
        assert_eq!(snap.selected_id.as_deref(), Some("box"));
    }

    #[test]
    fn batch_commits_trs_and_select() {
        let mut snap = demo_snap();
        apply_batch(
            &mut snap,
            &[
                AgentEditOp::Trs {
                    id: "box".into(),
                    translation: Some(Vec3::new(1.0, 0.675, 0.0)),
                    rotation: None,
                    scale: None,
                },
                AgentEditOp::Select {
                    id: "box".into(),
                },
            ],
            "agent:test",
            EditActorKind::Agent,
        )
        .unwrap();
        assert!((snap.project.scene.nodes["box"].transform.translation.x - 1.0).abs() < 1e-5);
        assert_eq!(snap.selected_id.as_deref(), Some("box"));
    }

    #[test]
    fn human_can_override_lock() {
        let mut snap = demo_snap();
        snap.project.locks.set(SemanticLock {
            node_id: "box".into(),
            scope: LockScope::Node,
            kind: LockKind::Semantic,
            holder: "user".into(),
        });
        apply_one(
            &mut snap,
            &AgentEditOp::Trs {
                id: "box".into(),
                translation: Some(Vec3::new(2.0, 0.675, 0.0)),
                rotation: None,
                scale: None,
            },
            "human:ui",
            EditActorKind::Human,
        )
        .unwrap();
        assert!((snap.project.scene.nodes["box"].transform.translation.x - 2.0).abs() < 1e-5);
    }

    #[test]
    fn camera_op_sets_look_at_and_fov() {
        let mut snap = demo_snap();
        apply_one(
            &mut snap,
            &AgentEditOp::Camera {
                id: None,
                eye: Some(Vec3::new(0.0, 3.0, 8.0)),
                look_at: Some(Vec3::new(0.0, 1.0, 0.0)),
                fov_deg: Some(35.0),
                yaw: None,
                pitch: None,
                distance: None,
            },
            "human:test",
            EditActorKind::Human,
        )
        .unwrap();
        let cam = &snap.project.scene.nodes["camera"];
        assert!((cam.transform.translation.z - 8.0).abs() < 1e-3);
        let params = cam.camera.as_ref().unwrap();
        let hfov = crate::camera::hfov_rad(params).to_degrees();
        assert!((hfov - 35.0).abs() < 0.5);
    }

    #[test]
    fn camera_op_denied_without_cap() {
        let err = require_edit_caps(
            &AgentEditOp::Camera {
                id: None,
                eye: None,
                look_at: None,
                fov_deg: Some(40.0),
                yaw: None,
                pitch: None,
                distance: None,
            },
            &[],
        )
        .unwrap_err();
        assert!(matches!(err, EditError::CapDenied(_)));
    }

    #[test]
    fn light_add_and_edit_intensity() {
        let mut snap = demo_snap();
        apply_one(
            &mut snap,
            &AgentEditOp::Light {
                id: Some("fill".into()),
                add: true,
                parent_id: None,
                name: Some("Fill".into()),
                translation: Some(Vec3::new(-2.0, 3.0, 1.0)),
                light_type: Some("directional".into()),
                intensity: Some(0.8),
                color: None,
                color_srgb: Some([180, 200, 255]),
                range: None,
                spot_angle_deg: None,
            },
            "human:ui",
            EditActorKind::Human,
        )
        .unwrap();
        assert_eq!(snap.project.scene.nodes["fill"].kind, NodeKind::Light);
        apply_one(
            &mut snap,
            &AgentEditOp::Light {
                id: Some("key_light".into()),
                add: false,
                parent_id: None,
                name: None,
                translation: None,
                light_type: None,
                intensity: Some(3.0),
                color: None,
                color_srgb: None,
                range: None,
                spot_angle_deg: None,
            },
            "human:ui",
            EditActorKind::Human,
        )
        .unwrap();
        assert!(
            (snap.project.scene.nodes["key_light"]
                .light
                .as_ref()
                .unwrap()
                .intensity
                - 3.0)
                .abs()
                < 1e-5
        );
    }

    #[test]
    fn light_op_denied_without_cap() {
        let err = require_edit_caps(
            &AgentEditOp::Light {
                id: None,
                add: true,
                parent_id: None,
                name: None,
                translation: None,
                light_type: None,
                intensity: None,
                color: None,
                color_srgb: None,
                range: None,
                spot_angle_deg: None,
            },
            &[],
        )
        .unwrap_err();
        assert!(matches!(err, EditError::CapDenied(_)));
    }
}
