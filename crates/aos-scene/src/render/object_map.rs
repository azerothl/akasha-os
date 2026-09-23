//! Geometry-based object ID map for selecting a rendered scene in the editor.

use crate::camera::{active_camera_eye_target, fovy_from_hfov, hfov_rad};
use crate::math::{Mat4, Vec3};
use crate::mesh_asset::{default_mesh_search_roots, load_gltf_mesh, resolve_mesh_uri};
use crate::png::encode_rgba8_png;
use crate::scene::{NodeKind, SceneGraph};

pub struct ObjectIdMap {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// RGB index 1 maps to the first node ID. Zero is the background.
    pub node_ids: Vec<String>,
    pub indices: Vec<u32>,
}

/// Rasterize visible mesh geometry from the same SceneGraph camera as the render.
/// Resolution is capped because the map is only used for pointer selection.
pub fn render_object_id_map(
    scene: &SceneGraph,
    image_width: u32,
    image_height: u32,
) -> Result<ObjectIdMap, String> {
    scene.validate().map_err(|e| e.to_string())?;
    if image_width == 0 || image_height == 0 {
        return Err("invalid render dimensions".into());
    }
    let factor = (1024.0 / image_width.max(image_height) as f32).min(1.0);
    let width = (image_width as f32 * factor).round().max(1.0) as u32;
    let height = (image_height as f32 * factor).round().max(1.0) as u32;
    let aspect = width as f32 / height as f32;
    let (eye, target) = active_camera_eye_target(scene, 8.0)
        .unwrap_or((Vec3::new(0.0, 1.5, 4.0), Vec3::new(0.0, 0.5, 0.0)));
    let (fovy, near, far) = scene
        .active_camera
        .as_deref()
        .and_then(|id| scene.nodes.get(id))
        .and_then(|node| node.camera.as_ref())
        .map(|camera| {
            (
                fovy_from_hfov(hfov_rad(camera), aspect),
                camera.near.max(0.01),
                camera.far.max(camera.near + 1.0),
            )
        })
        .unwrap_or((50.0_f32.to_radians(), 0.1, 200.0));
    let view_proj = perspective_rh(fovy, aspect, near, far) * look_at_rh(eye, target, Vec3::UNIT_Y);
    let mut depth = vec![f32::INFINITY; (width * height) as usize];
    let mut indices = vec![0u32; (width * height) as usize];
    let mut node_ids = Vec::new();
    let search_roots = default_mesh_search_roots();
    let roots: Vec<_> = search_roots.iter().map(|root| root.as_path()).collect();
    for id in scene.node_ids_depth_first() {
        let Some(node) = scene.nodes.get(&id) else {
            continue;
        };
        if !node.visible || !matches!(node.kind, NodeKind::MeshBox | NodeKind::MeshAsset) {
            continue;
        }
        let world = scene.world_matrix(&id).map_err(|e| e.to_string())?;
        let next_index = u32::try_from(node_ids.len() + 1).map_err(|e| e.to_string())?;
        node_ids.push(id);
        if matches!(node.kind, NodeKind::MeshAsset) {
            if let Some(mesh) = node
                .mesh_uri
                .as_deref()
                .and_then(|uri| resolve_mesh_uri(uri, &roots))
                .and_then(|path| load_gltf_mesh(&path).ok())
            {
                for tri in mesh.indices.as_chunks::<3>().0 {
                    let point = |index: u32| -> Option<Vec3> {
                        let offset = index as usize * 6;
                        let xyz = mesh.interleaved.get(offset..offset + 3)?;
                        Some(world.transform_point(Vec3::new(xyz[0], xyz[1], xyz[2])))
                    };
                    if let (Some(a), Some(b), Some(c)) =
                        (point(tri[0]), point(tri[1]), point(tri[2]))
                    {
                        raster_triangle(
                            [a, b, c],
                            next_index,
                            &view_proj,
                            width,
                            height,
                            &mut depth,
                            &mut indices,
                        );
                    }
                }
            }
            // A missing GLB has no reliable silhouette in the final image.
            // Never guess its hit region from the placeholder box.
            continue;
        }
        let corners = box_corners(&world);
        for [a, b, c] in BOX_TRIANGLES {
            raster_triangle(
                [corners[a], corners[b], corners[c]],
                next_index,
                &view_proj,
                width,
                height,
                &mut depth,
                &mut indices,
            );
        }
    }
    let mut rgba = vec![0u8; indices.len() * 4];
    for (pixel, index) in rgba.as_chunks_mut::<4>().0.iter_mut().zip(&indices) {
        pixel[0] = (index & 0xff) as u8;
        pixel[1] = ((index >> 8) & 0xff) as u8;
        pixel[2] = ((index >> 16) & 0xff) as u8;
        pixel[3] = 255;
    }
    Ok(ObjectIdMap {
        png: encode_rgba8_png(width, height, &rgba)?,
        width,
        height,
        node_ids,
        indices,
    })
}

const BOX_TRIANGLES: [[usize; 3]; 12] = [
    [0, 1, 2],
    [0, 2, 3],
    [4, 6, 5],
    [4, 7, 6],
    [0, 4, 5],
    [0, 5, 1],
    [1, 5, 6],
    [1, 6, 2],
    [2, 6, 7],
    [2, 7, 3],
    [3, 7, 4],
    [3, 4, 0],
];

fn box_corners(world: &Mat4) -> [Vec3; 8] {
    let v = [
        [-0.5, -0.5, -0.5],
        [0.5, -0.5, -0.5],
        [0.5, 0.5, -0.5],
        [-0.5, 0.5, -0.5],
        [-0.5, -0.5, 0.5],
        [0.5, -0.5, 0.5],
        [0.5, 0.5, 0.5],
        [-0.5, 0.5, 0.5],
    ];
    v.map(|p| world.transform_point(Vec3::new(p[0], p[1], p[2])))
}

fn project(point: Vec3, matrix: &Mat4, width: u32, height: u32) -> Option<[f32; 3]> {
    let m = &matrix.m;
    let x = m[0] * point.x + m[4] * point.y + m[8] * point.z + m[12];
    let y = m[1] * point.x + m[5] * point.y + m[9] * point.z + m[13];
    let z = m[2] * point.x + m[6] * point.y + m[10] * point.z + m[14];
    let w = m[3] * point.x + m[7] * point.y + m[11] * point.z + m[15];
    if w <= 0.001 {
        return None;
    }
    Some([
        (x / w * 0.5 + 0.5) * width as f32,
        (0.5 - y / w * 0.5) * height as f32,
        z / w,
    ])
}

fn raster_triangle(
    vertices: [Vec3; 3],
    id: u32,
    matrix: &Mat4,
    width: u32,
    height: u32,
    depth: &mut [f32],
    indices: &mut [u32],
) {
    let Some(a) = project(vertices[0], matrix, width, height) else {
        return;
    };
    let Some(b) = project(vertices[1], matrix, width, height) else {
        return;
    };
    let Some(c) = project(vertices[2], matrix, width, height) else {
        return;
    };
    let edge = |p: [f32; 3], q: [f32; 3], x: f32, y: f32| {
        (x - p[0]) * (q[1] - p[1]) - (y - p[1]) * (q[0] - p[0])
    };
    let area = edge(a, b, c[0], c[1]);
    if area.abs() < 1e-6 {
        return;
    }
    let min_x = a[0].min(b[0]).min(c[0]).floor().max(0.0) as u32;
    let max_x = a[0].max(b[0]).max(c[0]).ceil().min(width as f32) as u32;
    let min_y = a[1].min(b[1]).min(c[1]).floor().max(0.0) as u32;
    let max_y = a[1].max(b[1]).max(c[1]).ceil().min(height as f32) as u32;
    for y in min_y..max_y {
        for x in min_x..max_x {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let u = edge(b, c, px, py) / area;
            let v = edge(c, a, px, py) / area;
            let w = 1.0 - u - v;
            if u < 0.0 || v < 0.0 || w < 0.0 {
                continue;
            }
            let z = u * a[2] + v * b[2] + w * c[2];
            let offset = (y * width + x) as usize;
            if z < depth[offset] {
                depth[offset] = z;
                indices[offset] = id;
            }
        }
    }
}

fn look_at_rh(eye: Vec3, target: Vec3, up: Vec3) -> Mat4 {
    let f = (target - eye)
        .normalized()
        .unwrap_or(Vec3::new(0.0, 0.0, -1.0));
    let s = f.cross(up).normalized().unwrap_or(Vec3::UNIT_X);
    let u = s.cross(f);
    Mat4::from_cols(
        [s.x, u.x, -f.x, 0.0],
        [s.y, u.y, -f.y, 0.0],
        [s.z, u.z, -f.z, 0.0],
        [-s.dot(eye), -u.dot(eye), f.dot(eye), 1.0],
    )
}

fn perspective_rh(fovy: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let f = 1.0 / (fovy * 0.5).tan();
    let mut m = [0.0; 16];
    m[0] = f / aspect;
    m[5] = f;
    m[10] = (far + near) / (near - far);
    m[11] = -1.0;
    m[14] = 2.0 * far * near / (near - far);
    Mat4 { m }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{SceneNode, Transform};

    #[test]
    fn map_selects_a_front_box_and_leaves_background_empty() {
        let mut scene = SceneGraph {
            effects: Vec::new(),
            nodes: Default::default(),
            roots: vec!["box".into()],
            active_camera: None,
        };
        let mut node = SceneNode::empty("box", "Box");
        node.kind = NodeKind::MeshBox;
        node.transform = Transform::default();
        scene.nodes.insert("box".into(), node);
        let map = render_object_id_map(&scene, 100, 100).unwrap();
        assert_eq!(map.node_ids, ["box"]);
        assert_eq!(map.indices[50 * 100 + 50], 1);
        assert_eq!(map.indices[0], 0);
    }
}
