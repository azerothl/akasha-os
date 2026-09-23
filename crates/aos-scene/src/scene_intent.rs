//! Transient, typed scene planning. Only accepted candidates become SceneGraphs.

use crate::assets::{embedded_primitives_pack, instantiate_asset, AssetPack};
use crate::compose::{add_camera, add_key_light, empty_rooted_scene, ComposeResult};
use crate::math::{Quat, Vec3};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use thiserror::Error;

const MAX_OBJECTS: usize = 16;
const STAGE_LIMIT: f32 = 4.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneIntent {
    pub objects: Vec<IntentObject>,
    #[serde(default)]
    pub relations: Vec<SpatialRelation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntentObject {
    pub id: String,
    pub asset_id: String,
    #[serde(default)]
    pub attributes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationKind {
    On,
    InFrontOf,
    Near,
    Facing,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpatialRelation {
    pub subject: String,
    pub target: String,
    pub kind: RelationKind,
}

#[derive(Debug, Error, PartialEq)]
pub enum IntentError {
    #[error("invalid intent JSON: {0}")]
    Json(String),
    #[error("intent must contain 1 to {MAX_OBJECTS} objects")]
    ObjectCount,
    #[error("invalid or repeated object id: {0}")]
    ObjectId(String),
    #[error("asset is unavailable in the primitives pack: {0}")]
    Asset(String),
    #[error("relation must refer to an earlier target and a distinct subject: {0}")]
    Relation(String),
    #[error("no collision-free placement for: {0}")]
    Placement(String),
    #[error("scene: {0}")]
    Scene(String),
}

/// Parse a model answer without accepting prose, arbitrary fields or coordinates.
pub fn parse_scene_intent(raw: &str) -> Result<SceneIntent, IntentError> {
    let trimmed = raw.trim();
    let json = if let Some(inner) = trimmed.strip_prefix("```json") {
        inner.strip_suffix("```").unwrap_or(inner).trim()
    } else if let Some(inner) = trimmed.strip_prefix("```") {
        inner.strip_suffix("```").unwrap_or(inner).trim()
    } else {
        trimmed
    };
    serde_json::from_str(json).map_err(|e| IntentError::Json(e.to_string()))
}

pub fn plan_scene_intent(intent: &SceneIntent) -> Result<Vec<ComposeResult>, IntentError> {
    let pack = embedded_primitives_pack().map_err(|e| IntentError::Scene(e.to_string()))?;
    validate_intent(intent, &pack)?;
    let mut candidates = Vec::new();
    for seed in 0..2 {
        if let Ok(candidate) = solve(intent, &pack, seed) {
            candidates.push(candidate);
        }
    }
    if candidates.is_empty() {
        Err(IntentError::Placement("all objects".into()))
    } else {
        Ok(candidates)
    }
}

fn validate_intent(intent: &SceneIntent, pack: &AssetPack) -> Result<(), IntentError> {
    if intent.objects.is_empty() || intent.objects.len() > MAX_OBJECTS {
        return Err(IntentError::ObjectCount);
    }
    let mut ids = HashSet::new();
    for object in &intent.objects {
        if object.id.is_empty()
            || object.id.len() > 32
            || !object
                .id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_')
            || !ids.insert(object.id.as_str())
        {
            return Err(IntentError::ObjectId(object.id.clone()));
        }
        if pack.get(&object.asset_id).is_none() || footprint(&object.asset_id).is_none() {
            return Err(IntentError::Asset(object.asset_id.clone()));
        }
        if object.attributes.len() > 8 || object.attributes.iter().any(|a| a.len() > 80) {
            return Err(IntentError::ObjectId(object.id.clone()));
        }
    }
    if intent.relations.len() > MAX_OBJECTS * 2 {
        return Err(IntentError::Relation("too many relations".into()));
    }
    let indices: HashMap<_, _> = intent
        .objects
        .iter()
        .enumerate()
        .map(|(i, o)| (o.id.as_str(), i))
        .collect();
    for relation in &intent.relations {
        let subject = indices.get(relation.subject.as_str());
        let target = indices.get(relation.target.as_str());
        if !matches!((subject, target), (Some(s), Some(t)) if s > t) {
            return Err(IntentError::Relation(relation.subject.clone()));
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct Placed {
    position: Vec3,
    half_x: f32,
    half_z: f32,
    height: f32,
}

fn solve(
    intent: &SceneIntent,
    pack: &AssetPack,
    seed: usize,
) -> Result<ComposeResult, IntentError> {
    let mut scene = empty_rooted_scene();
    let mut occupied: HashMap<String, Placed> = HashMap::new();
    let mut placed_assets = Vec::new();
    let mut character_id = None;
    for object in &intent.objects {
        let (half_x, half_z, height) = footprint(&object.asset_id).expect("validated asset");
        let relations: Vec<_> = intent
            .relations
            .iter()
            .filter(|r| r.subject == object.id)
            .collect();
        let support = relations.iter().find(|r| r.kind == RelationKind::On);
        let placement = search_position(
            &object.id, half_x, half_z, height, &relations, &occupied, seed,
        )?;
        let instance = instantiate_asset(
            &mut scene,
            pack,
            &object.asset_id,
            Some("root"),
            &format!("intent_{}_", object.id),
        )
        .map_err(|e| IntentError::Scene(e.to_string()))?;
        let node = scene
            .nodes
            .get_mut(&instance.root_id)
            .expect("instantiated root");
        node.name = object.id.clone();
        node.transform.translation = placement.position;
        if let Some(target) = relations.iter().find(|r| r.kind == RelationKind::Facing) {
            let toward = occupied[&target.target].position - placement.position;
            node.transform.rotation =
                Quat::from_axis_angle(Vec3::UNIT_Y, toward.x.atan2(-toward.z));
        }
        if character_id.is_none()
            && (object.asset_id.starts_with("humanoid.")
                || object.asset_id.starts_with("quadruped."))
        {
            character_id = Some(instance.root_id.clone());
        }
        // A supporting surface may overlap in X/Z, but the new object sits above it.
        if support.is_some() && placement.position.y < 0.1 {
            return Err(IntentError::Placement(object.id.clone()));
        }
        occupied.insert(object.id.clone(), placement);
        placed_assets.push(object.asset_id.clone());
    }
    let camera_id = add_camera(&mut scene, Vec3::new(0.0, 2.4, 7.0))
        .map_err(|e| IntentError::Scene(e.to_string()))?;
    add_key_light(&mut scene).map_err(|e| IntentError::Scene(e.to_string()))?;
    scene
        .validate()
        .map_err(|e| IntentError::Scene(e.to_string()))?;
    Ok(ComposeResult {
        scene,
        template_id: format!("scene_intent_candidate_{}", seed + 1),
        placed_assets,
        character_id,
        camera_id,
    })
}

fn search_position(
    id: &str,
    half_x: f32,
    half_z: f32,
    height: f32,
    relations: &[&SpatialRelation],
    occupied: &HashMap<String, Placed>,
    seed: usize,
) -> Result<Placed, IntentError> {
    let on = relations.iter().find(|r| r.kind == RelationKind::On);
    let anchor = on
        .or_else(|| relations.iter().find(|r| r.kind == RelationKind::InFrontOf))
        .or_else(|| relations.iter().find(|r| r.kind == RelationKind::Near));
    let target = anchor.map(|r| occupied[&r.target]);
    let y = if on.is_some() {
        let support = target.expect("validated relation target");
        support.position.y + support.height
    } else {
        0.0
    };
    let base = match (anchor, target) {
        (Some(r), Some(t)) if r.kind == RelationKind::On => (t.position.x, t.position.z),
        (Some(r), Some(t)) if r.kind == RelationKind::InFrontOf => {
            (t.position.x, t.position.z + t.half_z + half_z + 0.25)
        }
        (Some(_), Some(t)) => (t.position.x + t.half_x + half_x + 0.35, t.position.z),
        _ => (if seed == 0 { -1.0 } else { 1.0 }, 0.0),
    };
    let offsets: &[(f32, f32)] = if on.is_some() {
        &[
            (0.0, 0.0),
            (-0.25, 0.0),
            (0.25, 0.0),
            (0.0, -0.25),
            (0.0, 0.25),
        ]
    } else if seed == 0 {
        &[
            (0.0, 0.0),
            (0.0, 1.0),
            (0.0, -1.0),
            (1.0, 0.0),
            (-1.0, 0.0),
            (1.5, 1.5),
            (-1.5, -1.5),
            (2.5, 0.0),
            (-2.5, 0.0),
        ]
    } else {
        &[
            (0.0, 0.0),
            (-1.0, 0.0),
            (1.0, 0.0),
            (0.0, -1.0),
            (0.0, 1.0),
            (-1.5, 1.5),
            (1.5, -1.5),
            (-2.5, 0.0),
            (2.5, 0.0),
        ]
    };
    for (dx, dz) in offsets {
        let x = base.0 + dx;
        let z = base.1 + dz;
        if let Some(support) = target.filter(|_| on.is_some()) {
            if (x - support.position.x).abs() + half_x > support.half_x
                || (z - support.position.z).abs() + half_z > support.half_z
            {
                continue;
            }
        }
        if x.abs() + half_x > STAGE_LIMIT || z.abs() + half_z > STAGE_LIMIT {
            continue;
        }
        let current = Placed {
            position: Vec3::new(x, y, z),
            half_x,
            half_z,
            height,
        };
        let collision = occupied.iter().any(|(other_id, other)| {
            if on.is_some_and(|r| r.target == *other_id) {
                return false;
            }
            (x - other.position.x).abs() < half_x + other.half_x + 0.08
                && (z - other.position.z).abs() < half_z + other.half_z + 0.08
                && y < other.position.y + other.height
                && other.position.y < y + height
        });
        if !collision {
            return Ok(current);
        }
    }
    Err(IntentError::Placement(id.into()))
}

fn footprint(asset: &str) -> Option<(f32, f32, f32)> {
    Some(match asset {
        "prop.box" => (0.35, 0.35, 0.7),
        "prop.chair" => (0.38, 0.38, 1.0),
        "prop.sofa" => (0.95, 0.42, 0.9),
        "prop.desk" | "prop.table" | "prop.counter" => (0.85, 0.45, 0.9),
        "prop.bookshelf" => (0.6, 0.25, 2.0),
        "prop.lamp" => (0.2, 0.2, 1.3),
        "prop.book" | "prop.cup" => (0.12, 0.12, 0.2),
        "prop.plant" => (0.3, 0.3, 1.2),
        "prop.pedestal" => (0.5, 0.5, 0.35),
        "prop.door" | "arch.window" => (0.5, 0.15, 2.1),
        "arch.wall" => (2.0, 0.12, 2.4),
        "arch.stairs" => (0.8, 0.8, 1.0),
        "humanoid.placeholder" | "humanoid.slim" | "humanoid.female" => (0.35, 0.3, 1.8),
        "quadruped.cat" => (0.35, 0.25, 0.5),
        "quadruped.dog" => (0.5, 0.35, 0.8),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_solves_on_near_facing() {
        let raw = r#"{"objects":[{"id":"desk","asset_id":"prop.desk"},{"id":"book","asset_id":"prop.book"},{"id":"woman","asset_id":"humanoid.female"}],"relations":[{"subject":"book","target":"desk","kind":"on"},{"subject":"woman","target":"desk","kind":"near"},{"subject":"woman","target":"book","kind":"facing"}]}"#;
        let intent = parse_scene_intent(raw).unwrap();
        let scenes = plan_scene_intent(&intent).unwrap();
        assert!(!scenes.is_empty());
        for candidate in scenes {
            candidate.scene.validate().unwrap();
            let desk = candidate
                .scene
                .nodes
                .values()
                .find(|n| n.name == "desk")
                .unwrap();
            let book = candidate
                .scene
                .nodes
                .values()
                .find(|n| n.name == "book")
                .unwrap();
            assert!(book.transform.translation.y > desk.transform.translation.y);
        }
    }

    #[test]
    fn rejects_untrusted_coordinates_and_invalid_assets() {
        assert!(parse_scene_intent(r#"{"objects":[],"coordinates":[1,2,3]}"#).is_err());
        let intent =
            parse_scene_intent(r#"{"objects":[{"id":"evil","asset_id":"../../secret"}]}"#).unwrap();
        assert_eq!(
            plan_scene_intent(&intent),
            Err(IntentError::Asset("../../secret".into()))
        );
    }

    #[test]
    fn rejects_relation_to_future_object() {
        let intent = parse_scene_intent(r#"{"objects":[{"id":"a","asset_id":"prop.book"},{"id":"b","asset_id":"prop.desk"}],"relations":[{"subject":"a","target":"b","kind":"on"}]}"#).unwrap();
        assert_eq!(
            plan_scene_intent(&intent),
            Err(IntentError::Relation("a".into()))
        );
    }

    #[test]
    fn rejects_object_too_large_for_support_and_separates_independent_objects() {
        let oversized = parse_scene_intent(r#"{"objects":[{"id":"book","asset_id":"prop.book"},{"id":"sofa","asset_id":"prop.sofa"}],"relations":[{"subject":"sofa","target":"book","kind":"on"}]}"#).unwrap();
        assert_eq!(
            plan_scene_intent(&oversized),
            Err(IntentError::Placement("all objects".into()))
        );
        let two = parse_scene_intent(r#"{"objects":[{"id":"one","asset_id":"prop.box"},{"id":"two","asset_id":"prop.box"}]}"#).unwrap();
        for candidate in plan_scene_intent(&two).unwrap() {
            let a = candidate
                .scene
                .nodes
                .values()
                .find(|n| n.name == "one")
                .unwrap();
            let b = candidate
                .scene
                .nodes
                .values()
                .find(|n| n.name == "two")
                .unwrap();
            assert!(
                (a.transform.translation.z - b.transform.translation.z).abs() >= 0.78
                    || (a.transform.translation.x - b.transform.translation.x).abs() >= 0.78
            );
        }
    }
}
