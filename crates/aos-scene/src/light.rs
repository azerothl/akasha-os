//! SceneGraph light helpers (ADR 0011).
//!
//! Color in the graph is **linear RGB**. DeclUI / agent boundaries may pass
//! sRGB 0–255 and convert here. Beauty backends (CPU, stub tint, Blender export)
//! consume [`collect_lights`].

use crate::math::{Vec3, EPSILON};
use crate::scene::{LightParams, LightType, NodeKind, SceneGraph};

/// Resolved world-space light for shading / export consumers.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedLight {
    pub id: String,
    pub light_type: LightType,
    pub position: Vec3,
    /// Unit direction the light shines (local −Z in world).
    pub direction: Vec3,
    pub intensity: f32,
    pub color: [f32; 3],
    pub range: f32,
    pub spot_angle_rad: f32,
}

/// Convert sRGB byte channel → linear (approx gamma 2.2).
pub fn srgb_u8_to_linear(c: u8) -> f32 {
    ((c as f32) / 255.0).powf(2.2)
}

/// Convert linear → sRGB byte.
pub fn linear_to_srgb_u8(c: f32) -> u8 {
    let v = c.max(0.0).powf(1.0 / 2.2);
    (v * 255.0).round().clamp(0.0, 255.0) as u8
}

/// Build linear RGB from DeclUI / agent sRGB bytes.
pub fn color_from_srgb_u8(rgb: [u8; 3]) -> [f32; 3] {
    [
        srgb_u8_to_linear(rgb[0]),
        srgb_u8_to_linear(rgb[1]),
        srgb_u8_to_linear(rgb[2]),
    ]
}

/// Parse light type from DeclUI / agent string (fail-closed → None).
pub fn parse_light_type(s: &str) -> Option<LightType> {
    match s.trim().to_ascii_lowercase().as_str() {
        "point" | "point_light" | "omni" => Some(LightType::Point),
        "directional" | "dir" | "sun" | "directionnel" => Some(LightType::Directional),
        "spot" | "spotlight" | "spot_light" => Some(LightType::Spot),
        _ => None,
    }
}

pub fn light_type_as_str(t: LightType) -> &'static str {
    match t {
        LightType::Point => "point",
        LightType::Directional => "directional",
        LightType::Spot => "spot",
    }
}

/// Collect visible Light nodes with world pose + params.
pub fn collect_lights(scene: &SceneGraph) -> Vec<ResolvedLight> {
    let mut out = Vec::new();
    for id in scene.node_ids_depth_first() {
        let Some(node) = scene.nodes.get(&id) else {
            continue;
        };
        if !node.visible || node.kind != NodeKind::Light {
            continue;
        }
        let params = node.light.clone().unwrap_or_default();
        let Ok(pos) = scene.world_translation(&id) else {
            continue;
        };
        let dir = scene
            .world_matrix(&id)
            .ok()
            .map(|m| {
                m.transform_vector(Vec3::new(0.0, 0.0, -1.0))
                    .normalized()
                    .unwrap_or(Vec3::new(0.0, -1.0, 0.0))
            })
            .unwrap_or(Vec3::new(0.0, -1.0, 0.0));
        out.push(ResolvedLight {
            id,
            light_type: params.light_type,
            position: pos,
            direction: dir,
            intensity: params.intensity.max(0.0),
            color: params.color,
            range: params.range.max(0.01),
            spot_angle_rad: params.spot_angle_rad.clamp(0.05, std::f32::consts::PI - 0.05),
        });
    }
    out
}

/// Diffuse irradiance at `point` with surface `normal` (world, unit preferred).
/// Returns linear RGB. Empty light list → `None` (caller keeps legacy flat shade).
pub fn shade_diffuse(
    lights: &[ResolvedLight],
    point: Vec3,
    normal: Vec3,
    albedo_linear: [f32; 3],
) -> Option<[f32; 3]> {
    if lights.is_empty() {
        return None;
    }
    let n = normal.normalized().unwrap_or(Vec3::UNIT_Y);
    let ambient = 0.14;
    let mut rgb = [
        albedo_linear[0] * ambient,
        albedo_linear[1] * ambient,
        albedo_linear[2] * ambient,
    ];
    for light in lights {
        let (l_dir, atten) = match light.light_type {
            LightType::Directional => (light.direction * -1.0, 1.0),
            LightType::Point => {
                let to_l = light.position - point;
                let dist = to_l.length().max(EPSILON);
                let dir = (to_l * (1.0 / dist)).normalized().unwrap_or(Vec3::UNIT_Y);
                let t = dist / light.range;
                let atten = 1.0 / (1.0 + t * t);
                (dir, atten)
            }
            LightType::Spot => {
                let to_l = light.position - point;
                let dist = to_l.length().max(EPSILON);
                let dir = (to_l * (1.0 / dist)).normalized().unwrap_or(Vec3::UNIT_Y);
                // Spot shines along local −Z (`direction`); surface sees opposite.
                let shine = light.direction * -1.0;
                let cos_outer = light.spot_angle_rad.cos();
                let cos_theta = shine.dot(dir * -1.0).clamp(-1.0, 1.0);
                if cos_theta < cos_outer {
                    continue;
                }
                let soft = ((cos_theta - cos_outer) / (1.0 - cos_outer).max(EPSILON)).clamp(0.0, 1.0);
                let t = dist / light.range;
                let atten = soft / (1.0 + t * t);
                (dir, atten)
            }
        };
        let ndotl = n.dot(l_dir).max(0.0);
        let w = light.intensity * atten * ndotl;
        rgb[0] += albedo_linear[0] * light.color[0] * w;
        rgb[1] += albedo_linear[1] * light.color[1] * w;
        rgb[2] += albedo_linear[2] * light.color[2] * w;
    }
    Some(rgb)
}

/// Average light color × intensity for stub / mock tint (cheap).
pub fn average_light_tint(lights: &[ResolvedLight]) -> Option<[u8; 3]> {
    if lights.is_empty() {
        return None;
    }
    let mut acc = [0.0f32; 3];
    let mut wsum = 0.0f32;
    for l in lights {
        let w = l.intensity.max(0.05);
        acc[0] += l.color[0] * w;
        acc[1] += l.color[1] * w;
        acc[2] += l.color[2] * w;
        wsum += w;
    }
    let inv = 1.0 / wsum.max(EPSILON);
    Some([
        linear_to_srgb_u8(acc[0] * inv),
        linear_to_srgb_u8(acc[1] * inv),
        linear_to_srgb_u8(acc[2] * inv),
    ])
}

/// Merge optional DeclUI / agent patches onto existing params.
pub fn merge_light_params(
    base: &LightParams,
    light_type: Option<LightType>,
    intensity: Option<f32>,
    color: Option<[f32; 3]>,
    range: Option<f32>,
    spot_angle_rad: Option<f32>,
) -> LightParams {
    let mut p = base.clone();
    if let Some(t) = light_type {
        p.light_type = t;
    }
    if let Some(i) = intensity {
        p.intensity = i;
    }
    if let Some(c) = color {
        p.color = c;
    }
    if let Some(r) = range {
        p.range = r;
    }
    if let Some(a) = spot_angle_rad {
        p.spot_angle_rad = a;
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{LightParams, SceneGraph, Transform};

    #[test]
    fn demo_scene_has_key_light() {
        let scene = SceneGraph::demo_scene();
        let lights = collect_lights(&scene);
        assert!(!lights.is_empty());
        assert!(lights.iter().any(|l| l.id == "key_light"));
    }

    #[test]
    fn shade_responds_to_intensity() {
        let mut scene = SceneGraph::demo_scene();
        let dim_params = LightParams {
            intensity: 0.2,
            ..LightParams::default()
        };
        scene
            .set_light_params("key_light", dim_params)
            .unwrap();
        let dim = shade_diffuse(
            &collect_lights(&scene),
            Vec3::ZERO,
            Vec3::UNIT_Y,
            [0.5, 0.5, 0.5],
        )
        .unwrap();
        let bright_params = LightParams {
            intensity: 4.0,
            ..LightParams::default()
        };
        scene
            .set_light_params("key_light", bright_params)
            .unwrap();
        let bright = shade_diffuse(
            &collect_lights(&scene),
            Vec3::ZERO,
            Vec3::UNIT_Y,
            [0.5, 0.5, 0.5],
        )
        .unwrap();
        assert!(bright[0] > dim[0]);
    }

    #[test]
    fn insert_light_unique() {
        let mut scene = SceneGraph::demo_scene();
        let id = scene
            .insert_light(
                "fill_light",
                "Fill",
                Some("root"),
                Transform {
                    translation: Vec3::new(-2.0, 3.0, 1.0),
                    ..Transform::default()
                },
                LightParams {
                    light_type: LightType::Directional,
                    intensity: 0.6,
                    color: [0.7, 0.8, 1.0],
                    ..LightParams::default()
                },
            )
            .unwrap();
        assert_eq!(id, "fill_light");
        assert_eq!(
            scene.nodes["fill_light"].light.as_ref().unwrap().light_type,
            LightType::Directional
        );
    }
}
