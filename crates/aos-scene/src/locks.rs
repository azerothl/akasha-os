//! Semantic locks for human + agent co-editing (Illustration Studio).
//!
//! Spec §131: a **semantic** lock is persistent user intent ("do not let the
//! agent change this node"). Distinct from short-lived editing locks (pointer
//! drag) which stay host-local and are out of scope here.
//!
//! Fail-closed for agents: any mutate that touches a locked node or its locked
//! ancestor/subtree is rejected. Humans may still unlock / override via UI.

use crate::scene::{SceneError, SceneGraph};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

/// Cap: set / clear semantic locks on SceneGraph nodes.
pub const SCENE_LOCK_CAP: &str = "scene.lock";

/// DeclUI / host_call: acquire a semantic lock.
pub const SCENE_LOCK_SERVICE: &str = "scene.lock";

/// DeclUI / host_call: release a semantic lock.
pub const SCENE_UNLOCK_SERVICE: &str = "scene.unlock";

/// DeclUI / host_call: list locks for the current project.
pub const SCENE_LOCKS_SERVICE: &str = "scene.locks";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LockScope {
    /// Only the named node.
    #[default]
    Node,
    /// Named node and all descendants.
    Subtree,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LockKind {
    /// General semantic lock — agent must not mutate the target.
    #[default]
    Semantic,
    /// Pose-only lock — agent may translate/rotate the root, not pose joints.
    Pose,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SemanticLock {
    pub node_id: String,
    #[serde(default)]
    pub scope: LockScope,
    #[serde(default)]
    pub kind: LockKind,
    /// Who set the lock (`user`, `agent:<id>`, …) — audit aid.
    #[serde(default = "default_holder")]
    pub holder: String,
}

fn default_holder() -> String {
    "user".into()
}

/// Project-level lock table (keyed by node id). Serialized under `locks:`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LockTable {
    #[serde(default, flatten)]
    pub entries: BTreeMap<String, LockEntryWire>,
}

/// Compact YAML form matching the draft spec (`camera: { semantic: true }`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LockEntryWire {
    #[serde(default)]
    pub semantic: bool,
    #[serde(default)]
    pub pose: bool,
    #[serde(default, skip_serializing_if = "is_node_scope")]
    pub scope: LockScope,
    #[serde(default = "default_holder", skip_serializing_if = "is_default_holder")]
    pub holder: String,
}

fn is_node_scope(s: &LockScope) -> bool {
    *s == LockScope::Node
}

fn is_default_holder(h: &str) -> bool {
    h == "user"
}

impl LockEntryWire {
    pub fn to_lock(&self, node_id: &str) -> Option<SemanticLock> {
        let kind = if self.pose {
            LockKind::Pose
        } else if self.semantic {
            LockKind::Semantic
        } else {
            return None;
        };
        Some(SemanticLock {
            node_id: node_id.to_string(),
            scope: self.scope,
            kind,
            holder: self.holder.clone(),
        })
    }

    pub fn from_lock(lock: &SemanticLock) -> Self {
        Self {
            semantic: lock.kind == LockKind::Semantic,
            pose: lock.kind == LockKind::Pose,
            scope: lock.scope,
            holder: lock.holder.clone(),
        }
    }
}

impl LockTable {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, node_id: &str) -> Option<SemanticLock> {
        self.entries
            .get(node_id)
            .and_then(|e| e.to_lock(node_id))
    }

    pub fn list(&self) -> Vec<SemanticLock> {
        self.entries
            .iter()
            .filter_map(|(id, e)| e.to_lock(id))
            .collect()
    }

    pub fn set(&mut self, lock: SemanticLock) {
        let id = lock.node_id.clone();
        self.entries.insert(id, LockEntryWire::from_lock(&lock));
    }

    pub fn remove(&mut self, node_id: &str) -> bool {
        self.entries.remove(node_id).is_some()
    }

    /// True when `node_id` is covered by any lock of `kind` (exact or ancestor subtree).
    pub fn blocks(
        &self,
        scene: &SceneGraph,
        node_id: &str,
        kind: LockKind,
    ) -> Result<Option<SemanticLock>, LockError> {
        if !scene.nodes.contains_key(node_id) {
            return Err(LockError::Scene(SceneError::UnknownNode(node_id.into())));
        }
        // Walk node → root; also check if any subtree lock covers this node.
        for lock in self.list() {
            if !kind_matches(lock.kind, kind) {
                continue;
            }
            match lock.scope {
                LockScope::Node => {
                    if lock.node_id == node_id {
                        return Ok(Some(lock));
                    }
                }
                LockScope::Subtree => {
                    if lock.node_id == node_id || is_descendant(scene, node_id, &lock.node_id) {
                        return Ok(Some(lock));
                    }
                }
            }
        }
        Ok(None)
    }

    /// Refuse agent mutation of `node_id` when a covering semantic/pose lock exists.
    pub fn assert_agent_may_mutate(
        &self,
        scene: &SceneGraph,
        node_id: &str,
        mutate: MutateKind,
    ) -> Result<(), LockError> {
        let needed = match mutate {
            MutateKind::Pose => LockKind::Pose,
            MutateKind::Transform | MutateKind::Structure => LockKind::Semantic,
        };
        if let Some(lock) = self.blocks(scene, node_id, needed)? {
            return Err(LockError::Blocked {
                node_id: node_id.into(),
                lock_node: lock.node_id,
                kind: lock.kind,
            });
        }
        // Pose lock also blocks pose ops; semantic lock blocks everything.
        if matches!(mutate, MutateKind::Pose) {
            if let Some(lock) = self.blocks(scene, node_id, LockKind::Semantic)? {
                return Err(LockError::Blocked {
                    node_id: node_id.into(),
                    lock_node: lock.node_id,
                    kind: lock.kind,
                });
            }
        }
        Ok(())
    }
}

fn kind_matches(lock_kind: LockKind, query: LockKind) -> bool {
    match query {
        LockKind::Semantic => lock_kind == LockKind::Semantic,
        LockKind::Pose => lock_kind == LockKind::Pose || lock_kind == LockKind::Semantic,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutateKind {
    Transform,
    Pose,
    Structure,
}

#[derive(Debug, Error, PartialEq)]
pub enum LockError {
    #[error("scene: {0}")]
    Scene(#[from] SceneError),
    #[error("unknown node `{0}`")]
    UnknownNode(String),
    #[error("node `{node_id}` blocked by {kind:?} lock on `{lock_node}`")]
    Blocked {
        node_id: String,
        lock_node: String,
        kind: LockKind,
    },
}

fn is_descendant(scene: &SceneGraph, id: &str, ancestor: &str) -> bool {
    let mut cur = scene.nodes.get(id).and_then(|n| n.parent.clone());
    let mut guard = 0usize;
    while let Some(pid) = cur {
        if pid == ancestor {
            return true;
        }
        if guard > scene.nodes.len() + 2 {
            return false;
        }
        guard += 1;
        cur = scene.nodes.get(&pid).and_then(|n| n.parent.clone());
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::SceneGraph;

    #[test]
    fn subtree_lock_blocks_descendant() {
        let scene = SceneGraph::demo_scene();
        let mut locks = LockTable::default();
        locks.set(SemanticLock {
            node_id: "humanoid".into(),
            scope: LockScope::Subtree,
            kind: LockKind::Semantic,
            holder: "user".into(),
        });
        let err = locks
            .assert_agent_may_mutate(&scene, "arm_r", MutateKind::Transform)
            .unwrap_err();
        assert!(matches!(err, LockError::Blocked { .. }));
        assert!(locks
            .assert_agent_may_mutate(&scene, "box", MutateKind::Transform)
            .is_ok());
    }

    #[test]
    fn pose_lock_blocks_pose_not_trs_on_sibling() {
        let scene = SceneGraph::demo_scene();
        let mut locks = LockTable::default();
        locks.set(SemanticLock {
            node_id: "humanoid".into(),
            scope: LockScope::Subtree,
            kind: LockKind::Pose,
            holder: "user".into(),
        });
        assert!(locks
            .assert_agent_may_mutate(&scene, "head", MutateKind::Pose)
            .is_err());
        // Root translation of a different node is fine.
        assert!(locks
            .assert_agent_may_mutate(&scene, "box", MutateKind::Transform)
            .is_ok());
    }

    #[test]
    fn wire_round_trip() {
        let mut table = LockTable::default();
        table.set(SemanticLock {
            node_id: "camera".into(),
            scope: LockScope::Node,
            kind: LockKind::Semantic,
            holder: "user".into(),
        });
        let yaml = serde_yaml::to_string(&table).unwrap();
        assert!(yaml.contains("camera:"));
        assert!(yaml.contains("semantic: true"));
        let loaded: LockTable = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(loaded.get("camera").unwrap().kind, LockKind::Semantic);
    }
}
