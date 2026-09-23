//! Undo-friendly SceneGraph ops (cheap foundation stack).

use crate::scene::{CameraParams, LightParams, NodeKind, SceneError, SceneGraph, Transform};
use serde::{Deserialize, Serialize};

const MAX_UNDO: usize = 64;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum SceneOp {
    /// Atomic multi-object edit (duplicate, group, alignment or snapping).
    ReplaceGraph {
        before: Box<SceneGraph>,
        after: Box<SceneGraph>,
    },
    SetMaterial {
        id: String,
        before: Option<crate::scene::MaterialOverride>,
        after: Option<crate::scene::MaterialOverride>,
    },
    SetTransform {
        id: String,
        before: Transform,
        after: Transform,
    },
    /// Camera transform + optics as one undo step.
    SetCamera {
        id: String,
        before: Transform,
        after: Transform,
        before_params: CameraParams,
        after_params: CameraParams,
    },
    /// Light transform + params as one undo step (existing node).
    SetLight {
        id: String,
        before: Transform,
        after: Transform,
        before_params: LightParams,
        after_params: LightParams,
    },
    /// Insert a new Light node (undo removes it).
    InsertLight {
        id: String,
        name: String,
        parent: String,
        transform: Transform,
        params: LightParams,
    },
    /// Remove a Light node (redo of InsertLight undo). Not constructed from DeclUI.
    RemoveLight { id: String },
    /// Multiple transforms as one undo step (pose presets / IK-lite).
    Batch { ops: Vec<SceneOp> },
}

impl SceneOp {
    pub fn apply(&self, graph: &mut SceneGraph) -> Result<(), SceneError> {
        match self {
            SceneOp::SetMaterial { id, after, .. } => {
                if let Some(material) = after {
                    material.validate()?;
                }
                graph
                    .nodes
                    .get_mut(id)
                    .ok_or_else(|| SceneError::UnknownNode(id.clone()))?
                    .material = after.clone();
                Ok(())
            }
            SceneOp::ReplaceGraph { after, .. } => {
                after.validate()?;
                *graph = (**after).clone();
                Ok(())
            }
            SceneOp::SetTransform { id, after, .. } => graph.set_transform(id, after.clone()),
            SceneOp::SetCamera {
                id,
                after,
                after_params,
                ..
            } => {
                graph.set_transform(id, after.clone())?;
                graph.set_camera_params(id, after_params.clone())
            }
            SceneOp::SetLight {
                id,
                after,
                after_params,
                ..
            } => {
                graph.set_transform(id, after.clone())?;
                graph.set_light_params(id, after_params.clone())
            }
            SceneOp::InsertLight {
                id,
                name,
                parent,
                transform,
                params,
            } => {
                graph.insert_light(
                    id,
                    name,
                    Some(parent.as_str()),
                    transform.clone(),
                    params.clone(),
                )?;
                Ok(())
            }
            SceneOp::RemoveLight { id } => remove_light_node(graph, id),
            SceneOp::Batch { ops } => {
                for op in ops {
                    op.apply(graph)?;
                }
                Ok(())
            }
        }
    }

    pub fn invert(&self) -> Self {
        match self {
            SceneOp::SetMaterial { id, before, after } => SceneOp::SetMaterial {
                id: id.clone(),
                before: after.clone(),
                after: before.clone(),
            },
            SceneOp::ReplaceGraph { before, after } => SceneOp::ReplaceGraph {
                before: after.clone(),
                after: before.clone(),
            },
            SceneOp::SetTransform { id, before, after } => SceneOp::SetTransform {
                id: id.clone(),
                before: after.clone(),
                after: before.clone(),
            },
            SceneOp::SetCamera {
                id,
                before,
                after,
                before_params,
                after_params,
            } => SceneOp::SetCamera {
                id: id.clone(),
                before: after.clone(),
                after: before.clone(),
                before_params: after_params.clone(),
                after_params: before_params.clone(),
            },
            SceneOp::SetLight {
                id,
                before,
                after,
                before_params,
                after_params,
            } => SceneOp::SetLight {
                id: id.clone(),
                before: after.clone(),
                after: before.clone(),
                before_params: after_params.clone(),
                after_params: before_params.clone(),
            },
            SceneOp::InsertLight { id, .. } => SceneOp::RemoveLight { id: id.clone() },
            // Redo after undo-of-insert: cannot rebuild without a snapshot.
            // UndoStack only pushes RemoveLight via InsertLight invert; redo of
            // that RemoveLight is the original InsertLight kept on the redo stack
            // (see UndoStack::undo — it stores the original op, not the invert).
            SceneOp::RemoveLight { id } => SceneOp::RemoveLight { id: id.clone() },
            SceneOp::Batch { ops } => SceneOp::Batch {
                ops: ops.iter().rev().map(SceneOp::invert).collect(),
            },
        }
    }
}

fn remove_light_node(graph: &mut SceneGraph, id: &str) -> Result<(), SceneError> {
    let node = graph
        .nodes
        .get(id)
        .ok_or_else(|| SceneError::UnknownNode(id.into()))?
        .clone();
    if node.kind != NodeKind::Light {
        return Err(SceneError::UnknownNode(id.into()));
    }
    if !node.children.is_empty() {
        return Err(SceneError::UnknownNode(format!(
            "light `{id}` has children"
        )));
    }
    if let Some(parent) = &node.parent {
        if let Some(p) = graph.nodes.get_mut(parent) {
            p.children.retain(|c| c != id);
        }
    }
    graph.nodes.remove(id);
    Ok(())
}

#[derive(Debug, Default)]
pub struct UndoStack {
    undo: Vec<SceneOp>,
    redo: Vec<SceneOp>,
}

impl UndoStack {
    pub fn push_apply(&mut self, graph: &mut SceneGraph, op: SceneOp) -> Result<(), SceneError> {
        op.apply(graph)?;
        self.record(op);
        Ok(())
    }

    /// Record an op that was already applied to the graph (pose builders).
    pub fn push_applied(&mut self, op: SceneOp) {
        self.record(op);
    }

    fn record(&mut self, op: SceneOp) {
        self.undo.push(op);
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn undo(&mut self, graph: &mut SceneGraph) -> Result<bool, SceneError> {
        let Some(op) = self.undo.pop() else {
            return Ok(false);
        };
        let inv = op.invert();
        inv.apply(graph)?;
        self.redo.push(op);
        Ok(true)
    }

    pub fn redo(&mut self, graph: &mut SceneGraph) -> Result<bool, SceneError> {
        let Some(op) = self.redo.pop() else {
            return Ok(false);
        };
        op.apply(graph)?;
        self.undo.push(op);
        Ok(true)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }
}
