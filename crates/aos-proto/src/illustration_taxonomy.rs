//! Cascaded pose taxonomy for Illustration (experiment T1–T4).
//!
//! Family → group → type with defaults at each level. Keyword classifier first;
//! Jevlike / regression are out of scope for this module.

use crate::{IllustrationSkeletonJoint, IllustrationSpec};
use serde::{Deserialize, Serialize};

const CATALOG_JSON: &str = include_str!("../data/illustration_pose_taxonomy.json");

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaxonomyCatalog {
    pub version: u32,
    pub families: Vec<TaxonomyFamily>,
    #[serde(default)]
    pub secondary_hints: Vec<SecondaryHint>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaxonomyFamily {
    pub id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub default_pose: Option<String>,
    #[serde(default)]
    pub groups: Vec<TaxonomyGroup>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaxonomyGroup {
    pub id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub default_pose: Option<String>,
    #[serde(default)]
    pub types: Vec<TaxonomyType>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaxonomyType {
    pub id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    pub pose: String,
    #[serde(default)]
    pub pose_if: Vec<PoseIf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PoseIf {
    pub any: Vec<String>,
    pub pose: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SecondaryHint {
    pub id: String,
    pub family: String,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
}

/// Result of cascading classification for one subject (or a secondary node).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ClassificationResult {
    pub family_id: String,
    #[serde(default)]
    pub group_id: Option<String>,
    #[serde(default)]
    pub type_id: Option<String>,
    pub confidence: f32,
    pub abstain: bool,
    #[serde(default)]
    pub pose_id: Option<String>,
    #[serde(default)]
    pub secondary_nodes: Vec<ClassificationResult>,
}

/// Instantiated skeleton for a taxonomy node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PoseInstance {
    pub node_id: String,
    pub pose_id: String,
    pub skeleton: Vec<IllustrationSkeletonJoint>,
    #[serde(default)]
    pub pose_tags: Vec<String>,
}

pub fn taxonomy_catalog() -> &'static TaxonomyCatalog {
    use std::sync::OnceLock;
    static CATALOG: OnceLock<TaxonomyCatalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(CATALOG_JSON).expect("illustration_pose_taxonomy.json must parse")
    })
}

fn hit(subject: &str, keywords: &[String]) -> bool {
    keywords
        .iter()
        .any(|k| !k.is_empty() && subject.contains(k))
}

fn hit_score(subject: &str, keywords: &[String]) -> usize {
    keywords
        .iter()
        .filter(|k| !k.is_empty() && subject.contains(k.as_str()))
        .count()
}

/// Keyword cascade: type > group > family. Unknown → abstain.
pub fn classify_illustration_subject(subject: &str) -> ClassificationResult {
    let subject = subject.to_ascii_lowercase();
    let catalog = taxonomy_catalog();

    let mut best: Option<(usize, ClassificationResult)> = None;
    for family in &catalog.families {
        if family.id == "unknown" {
            continue;
        }
        for group in &family.groups {
            for ty in &group.types {
                let score = hit_score(&subject, &ty.keywords);
                if score == 0 {
                    continue;
                }
                let pose_id = resolve_pose_id(&subject, ty);
                let cand = ClassificationResult {
                    family_id: family.id.clone(),
                    group_id: Some(group.id.clone()),
                    type_id: Some(ty.id.clone()),
                    confidence: (0.55 + 0.15 * score as f32).min(0.98),
                    abstain: false,
                    pose_id: Some(pose_id),
                    secondary_nodes: Vec::new(),
                };
                if best.as_ref().map(|(s, _)| *s).unwrap_or(0) < score + 20 {
                    best = Some((score + 20, cand));
                }
            }
            let gscore = hit_score(&subject, &group.keywords);
            if gscore > 0 && best.as_ref().map(|(s, _)| *s).unwrap_or(0) < gscore + 10 {
                best = Some((
                    gscore + 10,
                    ClassificationResult {
                        family_id: family.id.clone(),
                        group_id: Some(group.id.clone()),
                        type_id: None,
                        confidence: (0.4 + 0.1 * gscore as f32).min(0.85),
                        abstain: false,
                        pose_id: group.default_pose.clone().or(family.default_pose.clone()),
                        secondary_nodes: Vec::new(),
                    },
                ));
            }
        }
        let fscore = hit_score(&subject, &family.keywords);
        if fscore > 0 && best.as_ref().map(|(s, _)| *s).unwrap_or(0) < fscore {
            best = Some((
                fscore,
                ClassificationResult {
                    family_id: family.id.clone(),
                    group_id: None,
                    type_id: None,
                    confidence: (0.3 + 0.08 * fscore as f32).min(0.7),
                    abstain: false,
                    pose_id: family.default_pose.clone(),
                    secondary_nodes: Vec::new(),
                },
            ));
        }
    }

    let mut result = best.map(|(_, c)| c).unwrap_or(ClassificationResult {
        family_id: "unknown".into(),
        group_id: None,
        type_id: None,
        confidence: 0.0,
        abstain: true,
        pose_id: None,
        secondary_nodes: Vec::new(),
    });

    if !result.abstain {
        result.secondary_nodes = classify_secondaries(&subject, catalog, &result);
    }
    result
}

fn resolve_pose_id(subject: &str, ty: &TaxonomyType) -> String {
    for rule in &ty.pose_if {
        if rule.any.iter().any(|k| subject.contains(k)) {
            return rule.pose.clone();
        }
    }
    ty.pose.clone()
}

fn classify_secondaries(
    subject: &str,
    catalog: &TaxonomyCatalog,
    primary: &ClassificationResult,
) -> Vec<ClassificationResult> {
    let mut out = Vec::new();
    for hint in &catalog.secondary_hints {
        if !hit(subject, &hint.keywords) {
            continue;
        }
        // Avoid duplicating the primary subject as a secondary.
        if primary.type_id.as_deref() == Some(hint.id.as_str()) {
            continue;
        }
        let family = catalog.families.iter().find(|f| f.id == hint.family);
        let pose = secondary_pose_id(&hint.id).or_else(|| {
            family.and_then(|f| {
                if let Some(gid) = &hint.group {
                    f.groups
                        .iter()
                        .find(|g| g.id == *gid)
                        .and_then(|g| g.default_pose.clone())
                        .or(f.default_pose.clone())
                } else {
                    f.default_pose.clone()
                }
            })
        });
        out.push(ClassificationResult {
            family_id: hint.family.clone(),
            group_id: hint.group.clone(),
            type_id: Some(hint.id.clone()),
            confidence: 0.6,
            abstain: false,
            pose_id: pose,
            secondary_nodes: Vec::new(),
        });
    }
    out
}

fn secondary_pose_id(hint_id: &str) -> Option<String> {
    Some(
        match hint_id {
            "pipe" => "object_pipe",
            "seat" => "furniture_seat",
            "garden" => "nature_mass",
            _ => return None,
        }
        .into(),
    )
}

/// Build a pose instance from a classification (defaults climb the tree).
pub fn pose_instance_for(class: &ClassificationResult) -> Option<PoseInstance> {
    if class.abstain {
        return None;
    }
    let pose_id = class.pose_id.as_deref()?;
    let skeleton = skeleton_for_pose(pose_id)?;
    let node_id = class
        .type_id
        .clone()
        .or_else(|| class.group_id.clone())
        .unwrap_or_else(|| class.family_id.clone());
    let mut tags = vec![class.family_id.clone()];
    if let Some(g) = &class.group_id {
        tags.push(g.clone());
    }
    if let Some(t) = &class.type_id {
        tags.push(t.clone());
    }
    if pose_id.contains("sitting") {
        tags.push("sitting".into());
    }
    Some(PoseInstance {
        node_id,
        pose_id: pose_id.to_string(),
        skeleton,
        pose_tags: tags,
    })
}

fn joint(id: &str, parent: Option<&str>, x: f32, y: f32, radius: f32) -> IllustrationSkeletonJoint {
    IllustrationSkeletonJoint {
        id: id.into(),
        parent: parent.map(str::to_string),
        x,
        y,
        radius,
        ..Default::default()
    }
}

fn skeleton_for_pose(pose_id: &str) -> Option<Vec<IllustrationSkeletonJoint>> {
    Some(match pose_id {
        "human_sitting" => human_sitting(),
        "human_standing" => human_standing(),
        "quadruped_cat" => quadruped(0.50, 0.52, 0.92),
        "quadruped_dog" => quadruped(0.50, 0.50, 1.05),
        "quadruped_default" => quadruped(0.50, 0.52, 1.0),
        "furniture_seat" => furniture_seat(),
        "object_box" => object_box(),
        "object_pipe" => object_pipe(),
        "nature_mass" => nature_mass(),
        _ => return None,
    })
}

/// Eight-head drawing canon (crown = 0, sole = 8). One head is 0.09 of the sheet.
/// Pubis splits the height. Shoulders span two heads; male hips are narrower.
/// Upper arm is 1.5 heads, forearm 1.25 (wrist). Thigh equals shin. The foot is the last half head.
const HEAD: f32 = 0.09;
const CROWN_Y: f32 = 0.12;
const BODY_X: f32 = 0.46;

fn heads_y(heads: f32) -> f32 {
    CROWN_Y + heads * HEAD
}

fn human_standing() -> Vec<IllustrationSkeletonJoint> {
    let y = heads_y;
    vec![
        joint("pelvis", None, BODY_X, y(4.0), 0.032),
        joint("waist", Some("pelvis"), BODY_X, y(2.75), 0.024),
        joint("chest", Some("waist"), BODY_X, y(2.0), 0.038),
        joint("neck", Some("chest"), BODY_X, y(1.2), 0.02),
        joint("head", Some("neck"), BODY_X, y(0.5), 0.045),
        joint("shoulder_l", Some("chest"), BODY_X - HEAD, y(1.55), 0.02),
        joint(
            "elbow_l",
            Some("shoulder_l"),
            BODY_X - HEAD - 0.02,
            y(3.05),
            0.018,
        ),
        joint(
            "hand_l",
            Some("elbow_l"),
            BODY_X - HEAD - 0.025,
            y(4.3),
            0.016,
        ),
        joint("shoulder_r", Some("chest"), BODY_X + HEAD, y(1.55), 0.02),
        joint(
            "elbow_r",
            Some("shoulder_r"),
            BODY_X + HEAD + 0.02,
            y(3.05),
            0.018,
        ),
        joint(
            "hand_r",
            Some("elbow_r"),
            BODY_X + HEAD + 0.025,
            y(4.3),
            0.016,
        ),
        joint("hip_l", Some("pelvis"), BODY_X - 0.045, y(4.0), 0.02),
        joint("knee_l", Some("hip_l"), BODY_X - 0.05, y(5.75), 0.018),
        joint("ankle_l", Some("knee_l"), BODY_X - 0.048, y(7.5), 0.016),
        joint("foot_l", Some("ankle_l"), BODY_X - 0.02, y(8.0), 0.016),
        joint("hip_r", Some("pelvis"), BODY_X + 0.045, y(4.0), 0.02),
        joint("knee_r", Some("hip_r"), BODY_X + 0.05, y(5.75), 0.018),
        joint("ankle_r", Some("knee_r"), BODY_X + 0.048, y(7.5), 0.016),
        joint("foot_r", Some("ankle_r"), BODY_X + 0.07, y(8.0), 0.016),
    ]
}

fn human_sitting() -> Vec<IllustrationSkeletonJoint> {
    // Same bone lengths as standing. The torso stays upright. Both thighs reach
    // forward and both shins drop, so the legs do not cross. Elbows rest out
    // and the hands sit on the lap, below the shoulders.
    let pelvis_y = 0.58;
    let rise = pelvis_y - heads_y(4.0);
    let y = |heads: f32| heads_y(heads) + rise;
    vec![
        joint("pelvis", None, BODY_X, pelvis_y, 0.032),
        joint("waist", Some("pelvis"), BODY_X, y(2.75), 0.024),
        joint("chest", Some("waist"), BODY_X, y(2.0), 0.038),
        joint("neck", Some("chest"), BODY_X, y(1.2), 0.02),
        joint("head", Some("neck"), BODY_X, y(0.5), 0.045),
        joint("shoulder_l", Some("chest"), BODY_X - HEAD, y(1.55), 0.02),
        joint("elbow_l", Some("shoulder_l"), 0.316, 0.485, 0.018),
        joint("hand_l", Some("elbow_l"), 0.40, 0.56, 0.016),
        joint("shoulder_r", Some("chest"), BODY_X + HEAD, y(1.55), 0.02),
        joint("elbow_r", Some("shoulder_r"), 0.604, 0.485, 0.018),
        joint("hand_r", Some("elbow_r"), 0.52, 0.56, 0.016),
        joint("hip_l", Some("pelvis"), BODY_X - 0.045, pelvis_y, 0.02),
        joint("knee_l", Some("hip_l"), 0.561, 0.639, 0.018),
        joint("ankle_l", Some("knee_l"), 0.573, 0.796, 0.016),
        joint("foot_l", Some("ankle_l"), 0.621, 0.818, 0.016),
        joint("hip_r", Some("pelvis"), BODY_X + 0.045, pelvis_y, 0.02),
        joint("knee_r", Some("hip_r"), 0.657, 0.622, 0.018),
        joint("ankle_r", Some("knee_r"), 0.669, 0.779, 0.016),
        joint("foot_r", Some("ankle_r"), 0.717, 0.801, 0.016),
    ]
}

fn quadruped(cx: f32, cy: f32, scale: f32) -> Vec<IllustrationSkeletonJoint> {
    let s = scale;
    vec![
        joint("pelvis", None, cx + 0.10 * s, cy + 0.02 * s, 0.04),
        joint("chest", Some("pelvis"), cx - 0.06 * s, cy, 0.055),
        joint("neck", Some("chest"), cx - 0.14 * s, cy - 0.06 * s, 0.025),
        joint("head", Some("neck"), cx - 0.20 * s, cy - 0.12 * s, 0.055),
        joint(
            "shoulder_f",
            Some("chest"),
            cx - 0.10 * s,
            cy + 0.04 * s,
            0.022,
        ),
        joint(
            "paw_fl",
            Some("shoulder_f"),
            cx - 0.16 * s,
            cy + 0.16 * s,
            0.018,
        ),
        joint(
            "paw_fr",
            Some("shoulder_f"),
            cx - 0.04 * s,
            cy + 0.16 * s,
            0.018,
        ),
        joint("hip_b", Some("pelvis"), cx + 0.12 * s, cy + 0.04 * s, 0.022),
        joint("paw_bl", Some("hip_b"), cx + 0.08 * s, cy + 0.18 * s, 0.018),
        joint("paw_br", Some("hip_b"), cx + 0.18 * s, cy + 0.18 * s, 0.018),
        joint(
            "tail_root",
            Some("pelvis"),
            cx + 0.20 * s,
            cy - 0.02 * s,
            0.016,
        ),
        joint(
            "tail_tip",
            Some("tail_root"),
            cx + 0.28 * s,
            cy - 0.08 * s,
            0.014,
        ),
    ]
}

fn furniture_seat() -> Vec<IllustrationSkeletonJoint> {
    vec![
        joint("seat", None, 0.50, 0.68, 0.08),
        joint("back", Some("seat"), 0.38, 0.48, 0.05),
        joint("leg_fl", Some("seat"), 0.34, 0.82, 0.02),
        joint("leg_fr", Some("seat"), 0.62, 0.82, 0.02),
        joint("leg_bl", Some("seat"), 0.30, 0.80, 0.02),
        joint("leg_br", Some("seat"), 0.58, 0.80, 0.02),
    ]
}

fn object_box() -> Vec<IllustrationSkeletonJoint> {
    vec![
        joint("origin", None, 0.50, 0.55, 0.06),
        joint("top", Some("origin"), 0.50, 0.40, 0.04),
        joint("base", Some("origin"), 0.50, 0.70, 0.05),
    ]
}

fn object_pipe() -> Vec<IllustrationSkeletonJoint> {
    vec![
        joint("bowl", None, 0.68, 0.30, 0.022),
        joint("stem", Some("bowl"), 0.62, 0.325, 0.014),
        joint("grip", Some("stem"), 0.58, 0.33, 0.012),
    ]
}

fn nature_mass() -> Vec<IllustrationSkeletonJoint> {
    vec![
        joint("root", None, 0.50, 0.78, 0.04),
        joint("trunk", Some("root"), 0.50, 0.55, 0.035),
        joint("canopy", Some("trunk"), 0.50, 0.32, 0.08),
    ]
}

fn prefix_joints(
    prefix: &str,
    joints: Vec<IllustrationSkeletonJoint>,
) -> Vec<IllustrationSkeletonJoint> {
    joints
        .into_iter()
        .map(|j| IllustrationSkeletonJoint {
            id: format!("{prefix}{}", j.id),
            parent: j.parent.map(|p| format!("{prefix}{p}")),
            x: j.x,
            y: j.y,
            radius: j.radius,
            pitch: j.pitch,
            yaw: j.yaw,
            roll: j.roll,
        })
        .collect()
}

fn append_secondary_poses(spec: &mut IllustrationSpec, class: &ClassificationResult) {
    for secondary in &class.secondary_nodes {
        let Some(pose) = pose_instance_for(secondary) else {
            continue;
        };
        let prefix = format!("{}_", pose.node_id);
        // Skip if this secondary was already posed (idempotent enrich/compose).
        if spec.skeleton.iter().any(|j| j.id.starts_with(&prefix)) {
            continue;
        }
        let mut joints = prefix_joints(&prefix, pose.skeleton);
        // Nudge seat under pelvis / pipe near hand when primary joints exist.
        align_secondary_to_primary(&mut joints, &prefix, &spec.skeleton);
        spec.skeleton.extend(joints);
        insert_default_contacts(spec, &pose.node_id, &prefix);
    }
}

fn align_secondary_to_primary(
    joints: &mut [IllustrationSkeletonJoint],
    prefix: &str,
    primary: &[IllustrationSkeletonJoint],
) {
    let find = |id: &str| primary.iter().find(|j| j.id == id).map(|j| (j.x, j.y));
    if prefix == "seat_" {
        if let (Some((px, py)), Some(seat)) = (
            find("pelvis"),
            joints.iter_mut().find(|j| j.id == "seat_seat"),
        ) {
            let dx = px - seat.x;
            let dy = (py + 0.06) - seat.y;
            for j in joints.iter_mut() {
                j.x += dx;
                j.y += dy;
            }
        }
    } else if prefix == "pipe_" {
        if let (Some((hx, hy)), Some(grip)) = (
            find("hand_r").or_else(|| find("hand_l")),
            joints.iter_mut().find(|j| j.id == "pipe_grip"),
        ) {
            let dx = hx - grip.x;
            let dy = hy - grip.y;
            for j in joints.iter_mut() {
                j.x += dx;
                j.y += dy;
            }
        }
    } else if prefix == "garden_" {
        // Keep garden mass on the right/back; only shift if canopy would collide with head.
        if let Some((hx, _hy)) = find("head") {
            if let Some(canopy) = joints.iter().find(|j| j.id == "garden_canopy") {
                if (canopy.x - hx).abs() < 0.12 {
                    for j in joints.iter_mut() {
                        j.x = (j.x + 0.22).min(0.92);
                    }
                }
            }
        }
    }
}

fn insert_default_contacts(spec: &mut IllustrationSpec, node_id: &str, prefix: &str) {
    if node_id != "pipe" {
        return;
    }
    let grip = format!("{prefix}grip");
    if spec.skeleton.iter().any(|j| j.id == "hand_r")
        && spec.skeleton.iter().any(|j| j.id == grip)
        && spec.skeleton.iter().any(|j| j.id == "shoulder_r")
        && spec.skeleton.iter().any(|j| j.id == "elbow_r")
    {
        spec.contacts.entry("hold_pipe".into()).or_insert([
            "shoulder_r".into(),
            "elbow_r".into(),
            "hand_r".into(),
            grip,
        ]);
    }
}

/// Fill `spec.skeleton` (and secondary joints / contacts) from taxonomy when empty.
/// Returns the classification for callers/tests. Does not replace an authored skeleton.
pub fn apply_taxonomy_skeleton(spec: &mut IllustrationSpec) -> ClassificationResult {
    let subject = spec.brief.subject.to_ascii_lowercase();
    let class = classify_illustration_subject(&subject);
    if spec.skeleton.is_empty() {
        if let Some(pose) = pose_instance_for(&class) {
            spec.skeleton = pose.skeleton;
        }
    }
    if !class.abstain {
        append_secondary_poses(spec, &class);
    }
    class
}

/// True when classification is confident enough that enrich must not replace the puppet.
pub fn taxonomy_locks_puppet(class: &ClassificationResult) -> bool {
    !class.abstain && class.confidence >= 0.4 && class.pose_id.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{IllustrationBrief, IllustrationLook, IllustrationSpec};

    #[test]
    fn catalog_parses_and_has_core_families() {
        let c = taxonomy_catalog();
        assert!(c.families.iter().any(|f| f.id == "humanoid"));
        assert!(c.families.iter().any(|f| f.id == "mammal_quadruped"));
        assert!(c.families.iter().any(|f| f.id == "object"));
        assert!(c.families.iter().any(|f| f.id == "nature"));
    }

    #[test]
    fn gardener_classifies_to_sitting_human() {
        let c = classify_illustration_subject(
            "un vieux jardinier fumant une pipe dans un fauteuil regardant son jardin",
        );
        assert!(!c.abstain);
        assert_eq!(c.family_id, "humanoid");
        assert_eq!(c.type_id.as_deref(), Some("human"));
        assert_eq!(c.pose_id.as_deref(), Some("human_sitting"));
        assert!(c
            .secondary_nodes
            .iter()
            .any(|n| n.type_id.as_deref() == Some("pipe")));
        assert!(c
            .secondary_nodes
            .iter()
            .any(|n| n.type_id.as_deref() == Some("garden")));
        let pose = pose_instance_for(&c).expect("pose");
        assert!(pose.skeleton.iter().any(|j| j.id == "hand_r"));
        assert!(pose.skeleton.len() >= 12);
    }

    #[test]
    fn cat_classifies_to_carnivore_quadruped() {
        let c = classify_illustration_subject("un chat qui saute sur un canapé");
        assert!(!c.abstain);
        assert_eq!(c.family_id, "mammal_quadruped");
        assert_eq!(c.group_id.as_deref(), Some("carnivore"));
        assert_eq!(c.type_id.as_deref(), Some("cat"));
        let pose = pose_instance_for(&c).expect("pose");
        assert!(pose.skeleton.iter().any(|j| j.id == "paw_fl"));
    }

    #[test]
    fn ood_subject_abstains() {
        let c =
            classify_illustration_subject("un ornithorynque quantique sur un trampoline en plasma");
        assert!(c.abstain, "OOD must abstain rather than invent a type");
        assert_eq!(c.family_id, "unknown");
        assert!(pose_instance_for(&c).is_none());
    }

    #[test]
    fn apply_fills_empty_skeleton_and_keeps_authored() {
        let mut spec = IllustrationSpec {
            brief: IllustrationBrief {
                subject: "un homme assis sur un banc fumant une pipe".into(),
                look: IllustrationLook::Pencil,
                ..Default::default()
            },
            ..Default::default()
        };
        let class = apply_taxonomy_skeleton(&mut spec);
        assert!(!class.abstain);
        assert!(!spec.skeleton.is_empty());
        assert!(spec.skeleton.iter().any(|j| j.id.starts_with("pipe_")));
        assert!(spec.contacts.contains_key("hold_pipe"));
        let n = spec.skeleton.len();
        spec.skeleton[0].x = 0.11;
        let _ = apply_taxonomy_skeleton(&mut spec);
        assert_eq!(spec.skeleton.len(), n);
        assert!((spec.skeleton[0].x - 0.11).abs() < f32::EPSILON);
    }

    #[test]
    fn standing_human_follows_eight_head_canon() {
        let class = ClassificationResult {
            family_id: "humanoid".into(),
            pose_id: Some("human_standing".into()),
            ..ClassificationResult::default()
        };
        let pose = pose_instance_for(&class).expect("standing pose");
        let at = |id: &str| {
            pose.skeleton
                .iter()
                .find(|j| j.id == id)
                .map(|j| (j.x, j.y, j.radius))
                .unwrap_or_else(|| panic!("missing {id}"))
        };
        let dist = |a: (f32, f32, f32), b: (f32, f32, f32)| (a.0 - b.0).hypot(a.1 - b.1);
        let head = at("head");
        let crown = head.1 - head.2;
        let sole = at("foot_l").1;
        let head_h = head.2 * 2.0;
        assert!(
            ((sole - crown) / head_h - 8.0).abs() < 0.05,
            "crown-to-sole must be 8 heads"
        );
        assert!(
            (at("pelvis").1 - (crown + sole) * 0.5).abs() < 0.02,
            "pubis at mid-height"
        );
        let span = at("shoulder_r").0 - at("shoulder_l").0;
        assert!(
            (span / head_h - 2.0).abs() < 0.05,
            "shoulders span two heads"
        );
        let thigh = dist(at("hip_l"), at("knee_l"));
        let shin = dist(at("knee_l"), at("ankle_l"));
        assert!((thigh - shin).abs() < 0.01, "thigh equals shin");
        assert!(
            at("elbow_l").1 > at("shoulder_l").1,
            "hanging elbow is below the shoulder"
        );
        assert!(
            at("hand_l").1 > at("elbow_l").1,
            "hanging hand is below the elbow"
        );
        let upper = dist(at("shoulder_l"), at("elbow_l"));
        let fore = dist(at("elbow_l"), at("hand_l"));
        assert!(upper > fore, "upper arm longer than forearm");
        assert!((upper / head_h - 1.5).abs() < 0.15);
    }

    #[test]
    fn sitting_human_reaches_forward() {
        let pose_of = |pose_id: &str| {
            let class = ClassificationResult {
                family_id: "humanoid".into(),
                pose_id: Some(pose_id.into()),
                ..ClassificationResult::default()
            };
            pose_instance_for(&class).expect(pose_id).skeleton
        };
        let at = |pose: &[IllustrationSkeletonJoint], id: &str| {
            pose.iter()
                .find(|j| j.id == id)
                .map(|j| (j.x, j.y))
                .unwrap_or_else(|| panic!("missing {id}"))
        };
        let dist = |a: (f32, f32), b: (f32, f32)| (a.0 - b.0).hypot(a.1 - b.1);
        let sitting = pose_of("human_sitting");
        let standing = pose_of("human_standing");
        assert!(at(&sitting, "knee_l").0 > at(&sitting, "hip_l").0);
        assert!(at(&sitting, "knee_r").0 > at(&sitting, "hip_r").0);
        assert!(
            at(&sitting, "knee_l").0 < at(&sitting, "knee_r").0,
            "legs must not cross"
        );
        assert!(at(&sitting, "foot_l").0 < at(&sitting, "foot_r").0);
        assert!(at(&sitting, "elbow_l").1 > at(&sitting, "shoulder_l").1);
        assert!(at(&sitting, "elbow_r").1 > at(&sitting, "shoulder_r").1);
        assert!(at(&sitting, "elbow_l").0 < at(&sitting, "shoulder_l").0);
        assert!(at(&sitting, "elbow_r").0 > at(&sitting, "shoulder_r").0);
        assert!(at(&sitting, "hand_l").1 > at(&sitting, "elbow_l").1);
        assert!(at(&sitting, "hand_r").1 > at(&sitting, "elbow_r").1);
        assert!(at(&sitting, "hand_r").1 > at(&sitting, "chest").1);
        for bone in [
            ("shoulder_l", "elbow_l"),
            ("elbow_l", "hand_l"),
            ("hip_l", "knee_l"),
            ("knee_l", "ankle_l"),
        ] {
            let sit = dist(at(&sitting, bone.0), at(&sitting, bone.1));
            let stand = dist(at(&standing, bone.0), at(&standing, bone.1));
            assert!((sit - stand).abs() < 0.01, "{bone:?} length changed");
        }
    }

    #[test]
    fn gardener_secondary_poses_are_namespaced() {
        let mut spec = IllustrationSpec {
            brief: IllustrationBrief {
                subject: "un jardinier assis sur un fauteuil fumant une pipe dans son jardin"
                    .into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let class = apply_taxonomy_skeleton(&mut spec);
        assert!(class.secondary_nodes.len() >= 2);
        assert!(spec.skeleton.iter().any(|j| j.id == "pipe_grip"));
        assert!(spec.skeleton.iter().any(|j| j.id == "seat_seat"));
        assert!(spec.skeleton.iter().any(|j| j.id == "garden_canopy"));
    }
}
