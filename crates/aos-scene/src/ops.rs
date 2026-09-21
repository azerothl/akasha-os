//! Undo-friendly SceneGraph ops (cheap foundation stack).

use crate::scene::{SceneError, SceneGraph, Transform};
use serde::{Deserialize, Serialize};

const MAX_UNDO: usize = 64;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum SceneOp {
    SetTransform {
        id: String,
        before: Transform,
        after: Transform,
    },
    /// Multiple transforms as one undo step (pose presets / IK-lite).
    Batch {
        ops: Vec<SceneOp>,
    },
}

impl SceneOp {
    pub fn apply(&self, graph: &mut SceneGraph) -> Result<(), SceneError> {
        match self {
            SceneOp::SetTransform { id, after, .. } => graph.set_transform(id, after.clone()),
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
            SceneOp::SetTransform { id, before, after } => SceneOp::SetTransform {
                id: id.clone(),
                before: after.clone(),
                after: before.clone(),
            },
            SceneOp::Batch { ops } => SceneOp::Batch {
                ops: ops.iter().rev().map(SceneOp::invert).collect(),
            },
        }
    }
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
}
