//! Deterministic SceneGraph editing primitives used by the host viewport.

use crate::math::Vec3;
use crate::scene::{SceneError, SceneGraph, SceneNode};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditAxis {
    X,
    Y,
    Z,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignMode {
    Min,
    Center,
    Max,
}

fn selection<'a>(
    graph: &'a SceneGraph,
    ids: &'a [String],
) -> Result<Vec<&'a SceneNode>, SceneError> {
    if ids.is_empty() || ids.len() > 32 {
        return Err(SceneError::UnknownNode("select 1 to 32 objects".into()));
    }
    let mut seen = HashSet::new();
    let mut nodes = Vec::new();
    for id in ids {
        if !seen.insert(id) {
            continue;
        }
        let node = graph
            .nodes
            .get(id)
            .ok_or_else(|| SceneError::UnknownNode(id.clone()))?;
        nodes.push(node);
    }
    Ok(nodes)
}

fn unique_id(graph: &SceneGraph, reserved: &HashSet<String>, base: &str) -> String {
    for n in 1..=10000 {
        let id = format!("{base}_copy_{n}");
        if !graph.nodes.contains_key(&id) && !reserved.contains(&id) {
            return id;
        }
    }
    format!("{base}_copy_overflow")
}

pub fn duplicate_nodes(graph: &mut SceneGraph, ids: &[String]) -> Result<Vec<String>, SceneError> {
    graph.validate()?;
    let selected = selection(graph, ids)?;
    let selected_set: HashSet<_> = selected.iter().map(|n| n.id.as_str()).collect();
    let mut roots = Vec::new();
    for node in selected {
        let mut parent = node.parent.as_deref();
        let mut has_selected_ancestor = false;
        while let Some(id) = parent {
            if selected_set.contains(id) {
                has_selected_ancestor = true;
                break;
            }
            parent = graph.nodes.get(id).and_then(|n| n.parent.as_deref());
        }
        if !has_selected_ancestor {
            roots.push(node.id.clone());
        }
    }
    let mut originals = Vec::new();
    fn collect(graph: &SceneGraph, id: &str, out: &mut Vec<String>) {
        out.push(id.into());
        if let Some(node) = graph.nodes.get(id) {
            for child in &node.children {
                collect(graph, child, out);
            }
        }
    }
    for id in &roots {
        collect(graph, id, &mut originals);
    }
    let mut reserved = HashSet::new();
    let mut remap = HashMap::new();
    for id in &originals {
        let new_id = unique_id(graph, &reserved, id);
        reserved.insert(new_id.clone());
        remap.insert(id.clone(), new_id);
    }
    for id in &originals {
        let mut node = graph.nodes[id].clone();
        node.id = remap[id].clone();
        node.name = format!("{} copy", node.name);
        node.parent = node
            .parent
            .as_ref()
            .map(|p| remap.get(p).cloned().unwrap_or_else(|| p.clone()));
        node.children = node.children.iter().map(|c| remap[c].clone()).collect();
        if roots.contains(id) {
            node.transform.translation = node.transform.translation + Vec3::new(0.4, 0.0, 0.4);
        }
        graph.nodes.insert(node.id.clone(), node);
    }
    let copied_roots: Vec<_> = roots.iter().map(|id| remap[id].clone()).collect();
    for id in &copied_roots {
        if let Some(parent) = graph.nodes[id].parent.clone() {
            graph
                .nodes
                .get_mut(&parent)
                .expect("validated parent")
                .children
                .push(id.clone());
        } else {
            graph.roots.push(id.clone());
        }
    }
    graph.validate()?;
    Ok(copied_roots)
}

pub fn group_nodes(graph: &mut SceneGraph, ids: &[String]) -> Result<String, SceneError> {
    graph.validate()?;
    let nodes = selection(graph, ids)?;
    if nodes.len() < 2 {
        return Err(SceneError::UnknownNode(
            "select at least two objects".into(),
        ));
    }
    let parent = nodes[0].parent.clone();
    if nodes.iter().any(|n| n.parent != parent) {
        return Err(SceneError::UnknownNode(
            "group objects with the same parent".into(),
        ));
    }
    let center = nodes
        .iter()
        .fold(Vec3::ZERO, |sum, n| sum + n.transform.translation)
        * (1.0 / nodes.len() as f32);
    let group_id = unique_id(graph, &HashSet::new(), "group");
    let mut group = SceneNode::empty(&group_id, "Group");
    group.parent = parent.clone();
    group.children = nodes.iter().map(|n| n.id.clone()).collect();
    group.transform.translation = center;
    let selected: HashSet<_> = group.children.iter().cloned().collect();
    for id in &group.children {
        let node = graph.nodes.get_mut(id).expect("validated node");
        node.parent = Some(group_id.clone());
        node.transform.translation = node.transform.translation - center;
    }
    if let Some(parent) = parent {
        let p = graph.nodes.get_mut(&parent).expect("validated parent");
        p.children.retain(|c| !selected.contains(c));
        p.children.push(group_id.clone());
    } else {
        graph.roots.retain(|r| !selected.contains(r));
        graph.roots.push(group_id.clone());
    }
    graph.nodes.insert(group_id.clone(), group);
    graph.validate()?;
    Ok(group_id)
}

pub fn align_nodes(
    graph: &mut SceneGraph,
    ids: &[String],
    axis: EditAxis,
    mode: AlignMode,
) -> Result<(), SceneError> {
    let nodes = selection(graph, ids)?;
    if nodes.len() < 2 {
        return Err(SceneError::UnknownNode(
            "select at least two objects".into(),
        ));
    }
    if nodes.iter().any(|n| n.parent != nodes[0].parent) {
        return Err(SceneError::UnknownNode(
            "align objects with the same parent".into(),
        ));
    }
    let values: Vec<f32> = nodes
        .iter()
        .map(|n| axis_value(n.transform.translation, axis))
        .collect();
    let target = match mode {
        AlignMode::Min => values.iter().copied().fold(f32::INFINITY, f32::min),
        AlignMode::Center => values.iter().sum::<f32>() / values.len() as f32,
        AlignMode::Max => values.iter().copied().fold(f32::NEG_INFINITY, f32::max),
    };
    for id in ids {
        let node = graph.nodes.get_mut(id).expect("validated node");
        set_axis(&mut node.transform.translation, axis, target);
    }
    graph.validate()
}

pub fn snap_nodes(graph: &mut SceneGraph, ids: &[String], grid_m: f32) -> Result<(), SceneError> {
    selection(graph, ids)?;
    if !grid_m.is_finite() || !(0.01..=10.0).contains(&grid_m) {
        return Err(SceneError::UnknownNode("grid must be 0.01 to 10 m".into()));
    }
    for id in ids {
        let t = &mut graph
            .nodes
            .get_mut(id)
            .expect("validated node")
            .transform
            .translation;
        t.x = (t.x / grid_m).round() * grid_m;
        t.y = (t.y / grid_m).round() * grid_m;
        t.z = (t.z / grid_m).round() * grid_m;
    }
    graph.validate()
}

fn axis_value(v: Vec3, axis: EditAxis) -> f32 {
    match axis {
        EditAxis::X => v.x,
        EditAxis::Y => v.y,
        EditAxis::Z => v.z,
    }
}
fn set_axis(v: &mut Vec3, axis: EditAxis, value: f32) {
    match axis {
        EditAxis::X => v.x = value,
        EditAxis::Y => v.y = value,
        EditAxis::Z => v.z = value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::{SceneOp, UndoStack};

    #[test]
    fn duplicate_group_align_and_undo_preserve_scene() {
        let mut graph = SceneGraph::demo_scene();
        let before = graph.clone();
        let ids = vec!["box".into(), "pedestal".into()];
        let copies = duplicate_nodes(&mut graph, &ids).unwrap();
        assert_eq!(copies.len(), 2);
        let group = group_nodes(&mut graph, &copies).unwrap();
        assert_eq!(graph.nodes[&group].children.len(), 2);
        align_nodes(&mut graph, &copies, EditAxis::X, AlignMode::Center).unwrap();
        snap_nodes(&mut graph, &copies, 0.25).unwrap();
        let after = graph.clone();
        let mut undo = UndoStack::default();
        undo.push_applied(SceneOp::ReplaceGraph {
            before: Box::new(before.clone()),
            after: Box::new(after.clone()),
        });
        assert!(undo.undo(&mut graph).unwrap());
        assert_eq!(graph, before);
        assert!(undo.redo(&mut graph).unwrap());
        assert_eq!(graph, after);
    }

    #[test]
    fn two_transforms_undo_as_one_step() {
        let mut graph = SceneGraph::demo_scene();
        let before = graph.clone();
        let ops = ["box", "pedestal"]
            .iter()
            .map(|id| {
                let before = graph.nodes[*id].transform.clone();
                let mut after = before.clone();
                after.translation.x += 1.0;
                SceneOp::SetTransform {
                    id: (*id).into(),
                    before,
                    after,
                }
            })
            .collect();
        let mut undo = UndoStack::default();
        undo.push_apply(&mut graph, SceneOp::Batch { ops }).unwrap();
        assert_eq!(undo.undo_len(), 1);
        assert!(undo.undo(&mut graph).unwrap());
        assert_eq!(graph, before);
        assert!(undo.redo(&mut graph).unwrap());
        assert_eq!(
            graph.nodes["box"].transform.translation.x,
            before.nodes["box"].transform.translation.x + 1.0
        );
    }
}
