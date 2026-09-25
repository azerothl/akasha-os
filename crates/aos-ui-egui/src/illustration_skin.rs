//! Strict SkinTokens GLB grafting for Illustration Studio.
//!
//! SkinTokens currently exports skinning data without round-tripping every source
//! material/UV payload. This module copies only the skin, joints, and weight data
//! onto the original GLB after proving that both files have identical mesh
//! topology. The source file remains untouched and all its rendering data stays
//! byte-for-byte in place.

use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

const MAX_GLB_BYTES: u64 = 200 * 1024 * 1024;
const JSON_CHUNK: u32 = 0x4E4F534A;
const BIN_CHUNK: u32 = 0x004E4942;

#[derive(Debug, Clone, Copy)]
pub(crate) struct GraftedRigInfo {
    pub vertex_count: usize,
    pub triangle_count: usize,
    pub joint_count: usize,
}

struct Glb {
    json: Value,
    bin: Vec<u8>,
    other_chunks: Vec<(u32, Vec<u8>)>,
}

pub(crate) fn graft_skin(
    source_path: &Path,
    rig_path: &Path,
    output_path: &Path,
) -> Result<GraftedRigInfo, String> {
    let mut source = read_glb(source_path, "source")?;
    let rig = read_glb(rig_path, "SkinTokens")?;
    validate_single_mesh(&source.json, "source")?;
    validate_single_mesh(&rig.json, "SkinTokens")?;
    validate_buffers(&source, "source")?;
    validate_buffers(&rig, "SkinTokens")?;

    let (source_mesh_node, source_primitive) = single_mesh_node_primitive(&source.json, "source")?;
    let (rig_mesh_node, rig_primitive) = single_mesh_node_primitive(&rig.json, "SkinTokens")?;
    if source.json["nodes"][source_mesh_node].get("skin").is_some() {
        return Err(
            "The selected source GLB already has a skin; choose its unrigged mesh variant.".into(),
        );
    }
    if rig.json["nodes"][rig_mesh_node].get("skin").is_none() {
        return Err("SkinTokens output has no skin attached to its mesh node.".into());
    }
    if !same_node_transform(
        &source.json["nodes"][source_mesh_node],
        &rig.json["nodes"][rig_mesh_node],
    ) {
        return Err("SkinTokens changed the mesh-node transform. Texture-safe rigging requires the original mesh transform.".into());
    }

    let source_attrs = source.json["meshes"][0]["primitives"][0]["attributes"]
        .as_object()
        .ok_or_else(|| "Source GLB mesh has no vertex attributes.".to_string())?;
    let rig_attrs = rig.json["meshes"][0]["primitives"][0]["attributes"]
        .as_object()
        .ok_or_else(|| "SkinTokens mesh has no vertex attributes.".to_string())?;
    let source_position = accessor_index(source_attrs, "POSITION")?;
    let rig_position = accessor_index(rig_attrs, "POSITION")?;
    let source_positions = read_float_accessor(&source, source_position, "source POSITION")?;
    let rig_positions = read_float_accessor(&rig, rig_position, "SkinTokens POSITION")?;
    if source_positions != rig_positions {
        return Err("SkinTokens changed vertex positions. Exact texture preservation is unsafe, so no rigged asset was saved.".into());
    }
    let source_indices = read_indices(
        &source,
        &source.json["meshes"][0]["primitives"][source_primitive],
        source_positions.len(),
    )?;
    let rig_indices = read_indices(
        &rig,
        &rig.json["meshes"][0]["primitives"][rig_primitive],
        rig_positions.len(),
    )?;
    if source_indices != rig_indices {
        return Err("SkinTokens changed triangle indices. Exact texture preservation is unsafe, so no rigged asset was saved.".into());
    }

    let rig_skin_index = rig.json["nodes"][rig_mesh_node]["skin"]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| "SkinTokens mesh references an invalid skin.".to_string())?;
    let rig_skins = rig.json["skins"]
        .as_array()
        .ok_or_else(|| "SkinTokens output has no skins array.".to_string())?;
    let rig_skin = rig_skins
        .get(rig_skin_index)
        .ok_or_else(|| "SkinTokens mesh references a missing skin.".to_string())?;
    let joints = rig_skin["joints"]
        .as_array()
        .ok_or_else(|| "SkinTokens skin has no joints.".to_string())?;
    if joints.is_empty() || joints.len() > 1024 {
        return Err("SkinTokens skin has an unsupported joint count.".into());
    }
    let joint_count = joints.len();
    if let Some(index) = rig_skin["inverseBindMatrices"].as_u64() {
        let index = usize::try_from(index).map_err(|_| "Invalid inverse bind accessor index.")?;
        let accessor = rig.json["accessors"]
            .get(index)
            .ok_or_else(|| "SkinTokens inverse bind accessor is missing.".to_string())?;
        if accessor["type"].as_str() != Some("MAT4")
            || accessor["componentType"].as_u64() != Some(5126)
            || accessor["count"].as_u64() != Some(joint_count as u64)
        {
            return Err("SkinTokens inverse bind matrices do not match its joint count.".into());
        }
        let _ = accessor_values(&rig, index, "SkinTokens inverse bind matrices")?;
    }

    let rig_joint_accessor = accessor_index(rig_attrs, "JOINTS_0")?;
    let rig_weight_accessor = accessor_index(rig_attrs, "WEIGHTS_0")?;
    let joint_values = read_unsigned_accessor(&rig, rig_joint_accessor, "SkinTokens JOINTS_0")?;
    let weight_values = read_weight_accessor(&rig, rig_weight_accessor)?;
    if joint_values.len() != source_positions.len() || weight_values.len() != source_positions.len()
    {
        return Err("SkinTokens skin weights do not match the source vertex count.".into());
    }
    if joint_values
        .iter()
        .flatten()
        .any(|joint| *joint as usize >= joint_count)
    {
        return Err("SkinTokens weights reference a joint missing from its skeleton.".into());
    }
    if weight_values
        .iter()
        .any(|weights| weights.iter().sum::<f64>() <= 0.0)
    {
        return Err("SkinTokens produced vertices with no usable skin weights.".into());
    }

    let rig_nodes = rig.json["nodes"]
        .as_array()
        .ok_or_else(|| "SkinTokens output has no nodes array.".to_string())?;
    let included_nodes = skeleton_node_closure(rig_nodes, rig_skin)?;
    let source_nodes = source.json["nodes"]
        .as_array()
        .ok_or_else(|| "Source GLB has no nodes array.".to_string())?;
    let source_node_count = source_nodes.len();
    let mut node_remap = HashMap::new();
    for index in included_nodes.iter().copied() {
        node_remap.insert(index, source_node_count + node_remap.len());
    }

    let mut cloned_nodes = Vec::with_capacity(included_nodes.len());
    let included: HashSet<usize> = included_nodes.iter().copied().collect();
    for old_index in included_nodes.iter().copied() {
        let mut node = rig_nodes[old_index].clone();
        if node.get("mesh").is_some() || node.get("camera").is_some() || node.get("skin").is_some()
        {
            return Err("SkinTokens skeleton hierarchy contains a non-joint scene object.".into());
        }
        if let Some(object) = node.as_object_mut() {
            if let Some(children) = object.get_mut("children").and_then(Value::as_array_mut) {
                let mapped = children
                    .iter()
                    .filter_map(|child| {
                        let old = child.as_u64().and_then(|n| usize::try_from(n).ok())?;
                        if included.contains(&old) {
                            node_remap
                                .get(&old)
                                .map(|mapped| Value::from(*mapped as u64))
                        } else {
                            None
                        }
                    })
                    .collect();
                *children = mapped;
            }
        }
        cloned_nodes.push(node);
    }
    source.json["nodes"]
        .as_array_mut()
        .expect("checked nodes")
        .extend(cloned_nodes);

    let roots: Vec<usize> = included_nodes
        .iter()
        .copied()
        .filter(|index| {
            !rig_nodes.iter().any(|node| {
                node.get("children")
                    .and_then(Value::as_array)
                    .is_some_and(|children| {
                        children
                            .iter()
                            .any(|child| child.as_u64() == Some(*index as u64))
                    })
            })
        })
        .collect();
    let scene_index = source
        .json
        .get("scene")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let scenes = source.json["scenes"]
        .as_array_mut()
        .ok_or_else(|| "Source GLB has no scenes array.".to_string())?;
    let active_scene = scenes
        .get_mut(scene_index)
        .ok_or_else(|| "Source GLB has an invalid active scene.".to_string())?;
    let scene_roots = active_scene["nodes"]
        .as_array_mut()
        .ok_or_else(|| "Source GLB active scene has no root nodes.".to_string())?;
    for root in roots {
        if let Some(mapped) = node_remap.get(&root) {
            scene_roots.push(Value::from(*mapped as u64));
        }
    }

    let mut view_remap = HashMap::new();
    let joints_accessor = copy_accessor(&mut source, &rig, rig_joint_accessor, &mut view_remap)?;
    let weights_accessor = copy_accessor(&mut source, &rig, rig_weight_accessor, &mut view_remap)?;
    let inverse_bind_accessor = if let Some(index) = rig_skin["inverseBindMatrices"].as_u64() {
        let index = usize::try_from(index)
            .map_err(|_| "Invalid inverse bind accessor index.".to_string())?;
        Some(copy_accessor(&mut source, &rig, index, &mut view_remap)?)
    } else {
        None
    };

    let mut skin = rig_skin.clone();
    let mapped_joints: Vec<Value> = joints
        .iter()
        .map(|joint| {
            let old = joint.as_u64().and_then(|n| usize::try_from(n).ok())?;
            node_remap
                .get(&old)
                .copied()
                .map(|mapped| Value::from(mapped as u64))
        })
        .collect::<Option<_>>()
        .ok_or_else(|| "SkinTokens skin references a node outside its skeleton.".to_string())?;
    skin["joints"] = Value::Array(mapped_joints);
    if let Some(skeleton) = rig_skin.get("skeleton").and_then(Value::as_u64) {
        let old =
            usize::try_from(skeleton).map_err(|_| "Invalid skeleton root index.".to_string())?;
        skin["skeleton"] = Value::from(
            *node_remap
                .get(&old)
                .ok_or_else(|| "SkinTokens skeleton root is missing.".to_string())?
                as u64,
        );
    }
    match inverse_bind_accessor {
        Some(index) => skin["inverseBindMatrices"] = Value::from(index as u64),
        None => {
            skin.as_object_mut()
                .map(|object| object.remove("inverseBindMatrices"));
        }
    }
    let skins = source
        .json
        .as_object_mut()
        .and_then(|root| {
            root.entry("skins")
                .or_insert_with(|| Value::Array(Vec::new()))
                .as_array_mut()
        })
        .ok_or_else(|| "Source GLB skins field is invalid.".to_string())?;
    let output_skin_index = skins.len();
    skins.push(skin);
    source.json["nodes"][source_mesh_node]["skin"] = Value::from(output_skin_index as u64);
    source.json["meshes"][0]["primitives"][0]["attributes"]["JOINTS_0"] =
        Value::from(joints_accessor as u64);
    source.json["meshes"][0]["primitives"][0]["attributes"]["WEIGHTS_0"] =
        Value::from(weights_accessor as u64);
    if let Some(buffers) = source.json["buffers"].as_array_mut() {
        if let Some(buffer) = buffers.get_mut(0) {
            buffer["byteLength"] = Value::from(source.bin.len() as u64);
        }
    }

    let triangle_count = source_indices.len() / 3;
    if source_indices.len() % 3 != 0 {
        return Err("Source GLB indices are not a complete triangle list.".into());
    }
    write_glb(&source, output_path)?;
    Ok(GraftedRigInfo {
        vertex_count: source_positions.len(),
        triangle_count,
        joint_count,
    })
}

fn read_glb(path: &Path, label: &str) -> Result<Glb, String> {
    let metadata = fs::metadata(path).map_err(|e| format!("Could not read {label} GLB: {e}"))?;
    if metadata.len() == 0 || metadata.len() > MAX_GLB_BYTES {
        return Err(format!(
            "{label} GLB must be non-empty and smaller than 200 MB."
        ));
    }
    let bytes = fs::read(path).map_err(|e| format!("Could not read {label} GLB: {e}"))?;
    if bytes.len() < 20 || &bytes[..4] != b"glTF" || read_u32(&bytes, 4)? != 2 {
        return Err(format!("{label} output is not a glTF 2.0 binary GLB."));
    }
    if read_u32(&bytes, 8)? as usize != bytes.len() {
        return Err(format!(
            "{label} GLB header length does not match its file size."
        ));
    }
    let mut cursor = 12usize;
    let mut json = None;
    let mut bin = Vec::new();
    let mut other_chunks = Vec::new();
    while cursor < bytes.len() {
        let chunk_len = read_u32(&bytes, cursor)? as usize;
        let chunk_type = read_u32(&bytes, cursor + 4)?;
        cursor = cursor.checked_add(8).ok_or("GLB chunk offset overflow")?;
        let end = cursor
            .checked_add(chunk_len)
            .filter(|end| *end <= bytes.len())
            .ok_or_else(|| format!("{label} GLB has an invalid chunk length."))?;
        match chunk_type {
            JSON_CHUNK if json.is_none() => {
                let chunk = bytes[cursor..end]
                    .iter()
                    .copied()
                    .take_while(|byte| *byte != 0)
                    .collect::<Vec<_>>();
                let trimmed = chunk
                    .iter()
                    .rposition(|byte| *byte != b' ')
                    .map_or(0, |index| index + 1);
                let chunk = &chunk[..trimmed];
                json = Some(
                    serde_json::from_slice(chunk)
                        .map_err(|e| format!("{label} GLB JSON is invalid: {e}"))?,
                );
            }
            BIN_CHUNK if bin.is_empty() => bin.extend_from_slice(&bytes[cursor..end]),
            JSON_CHUNK | BIN_CHUNK => {
                return Err(format!("{label} GLB contains a duplicate standard chunk."))
            }
            chunk_type => other_chunks.push((chunk_type, bytes[cursor..end].to_vec())),
        }
        cursor = end;
    }
    let json = json.ok_or_else(|| format!("{label} GLB has no JSON chunk."))?;
    Ok(Glb {
        json,
        bin,
        other_chunks,
    })
}

fn write_glb(glb: &Glb, path: &Path) -> Result<(), String> {
    let mut json = serde_json::to_vec(&glb.json)
        .map_err(|e| format!("Could not encode rigged GLB JSON: {e}"))?;
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    let mut bin = glb.bin.clone();
    while !bin.len().is_multiple_of(4) {
        bin.push(0);
    }
    let extra_len = glb
        .other_chunks
        .iter()
        .try_fold(0usize, |total, (_, chunk)| {
            total.checked_add(8 + chunk.len())
        })
        .ok_or_else(|| "GLB chunk length overflow.".to_string())?;
    let total = 12usize
        .checked_add(8 + json.len())
        .and_then(|n| n.checked_add(8 + bin.len()))
        .and_then(|n| n.checked_add(extra_len))
        .filter(|n| *n <= MAX_GLB_BYTES as usize && *n <= u32::MAX as usize)
        .ok_or_else(|| "Rigged GLB would exceed the 200 MB size limit.".to_string())?;
    let mut bytes = Vec::with_capacity(total);
    bytes.extend_from_slice(b"glTF");
    bytes.extend_from_slice(&2u32.to_le_bytes());
    bytes.extend_from_slice(&(total as u32).to_le_bytes());
    bytes.extend_from_slice(&(json.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&JSON_CHUNK.to_le_bytes());
    bytes.extend_from_slice(&json);
    bytes.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&BIN_CHUNK.to_le_bytes());
    bytes.extend_from_slice(&bin);
    for (chunk_type, chunk) in &glb.other_chunks {
        if chunk.len() % 4 != 0 || chunk.len() > u32::MAX as usize {
            return Err("Source GLB contains an invalid extension chunk.".into());
        }
        bytes.extend_from_slice(&(chunk.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&chunk_type.to_le_bytes());
        bytes.extend_from_slice(chunk);
    }
    fs::write(path, bytes).map_err(|e| format!("Could not save rigged GLB: {e}"))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let slice = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| "GLB is truncated.".to_string())?;
    Ok(u32::from_le_bytes(
        slice.try_into().map_err(|_| "GLB is truncated.")?,
    ))
}

fn validate_buffers(glb: &Glb, label: &str) -> Result<(), String> {
    let buffers = glb.json["buffers"]
        .as_array()
        .ok_or_else(|| format!("{label} GLB has no buffer."))?;
    if buffers.len() != 1 || buffers[0].get("uri").is_some() {
        return Err(format!("{label} GLB must use one embedded binary buffer."));
    }
    let declared = buffers[0]["byteLength"]
        .as_u64()
        .ok_or_else(|| format!("{label} GLB has an invalid buffer length."))?;
    if declared as usize > glb.bin.len() {
        return Err(format!("{label} GLB binary buffer is truncated."));
    }
    Ok(())
}

fn validate_single_mesh(json: &Value, label: &str) -> Result<(), String> {
    let meshes = json["meshes"]
        .as_array()
        .ok_or_else(|| format!("{label} GLB has no mesh."))?;
    if meshes.len() != 1
        || meshes[0]["primitives"]
            .as_array()
            .is_none_or(|items| items.len() != 1)
    {
        return Err(format!(
            "{label} GLB must contain one mesh with one primitive for texture-safe rigging."
        ));
    }
    let mode = meshes[0]["primitives"][0]
        .get("mode")
        .and_then(Value::as_u64)
        .unwrap_or(4);
    if mode != 4 {
        return Err(format!("{label} GLB mesh must use triangle primitives."));
    }
    Ok(())
}

fn single_mesh_node_primitive(json: &Value, label: &str) -> Result<(usize, usize), String> {
    let nodes = json["nodes"]
        .as_array()
        .ok_or_else(|| format!("{label} GLB has no nodes."))?;
    let matches: Vec<usize> = nodes
        .iter()
        .enumerate()
        .filter_map(|(index, node)| {
            (node.get("mesh").and_then(Value::as_u64) == Some(0)).then_some(index)
        })
        .collect();
    if matches.len() != 1 {
        return Err(format!(
            "{label} GLB must have exactly one node for its mesh."
        ));
    }
    Ok((matches[0], 0))
}

fn same_node_transform(left: &Value, right: &Value) -> bool {
    let (Some(left), Some(right)) = (node_transform(left), node_transform(right)) else {
        return false;
    };
    left.iter()
        .zip(right.iter())
        .all(|(a, b)| (a - b).abs() <= 1e-6)
}

fn node_transform(node: &Value) -> Option<[f64; 16]> {
    if let Some(matrix) = node.get("matrix") {
        let values: [f64; 16] = numeric_array(matrix)?.try_into().ok()?;
        return values
            .iter()
            .all(|value| value.is_finite())
            .then_some(values);
    }
    let translation = match node.get("translation") {
        Some(value) => numeric_array(value)?,
        None => vec![0.0, 0.0, 0.0],
    };
    let rotation = match node.get("rotation") {
        Some(value) => numeric_array(value)?,
        None => vec![0.0, 0.0, 0.0, 1.0],
    };
    let scale = match node.get("scale") {
        Some(value) => numeric_array(value)?,
        None => vec![1.0, 1.0, 1.0],
    };
    if translation.len() != 3
        || rotation.len() != 4
        || scale.len() != 3
        || translation
            .iter()
            .chain(rotation.iter())
            .chain(scale.iter())
            .any(|value| !value.is_finite())
    {
        return None;
    }
    let [tx, ty, tz] = <[f64; 3]>::try_from(translation).ok()?;
    let [x, y, z, w] = <[f64; 4]>::try_from(rotation).ok()?;
    let [sx, sy, sz] = <[f64; 3]>::try_from(scale).ok()?;
    let xx = x * x;
    let yy = y * y;
    let zz = z * z;
    let xy = x * y;
    let xz = x * z;
    let yz = y * z;
    let wx = w * x;
    let wy = w * y;
    let wz = w * z;
    Some([
        (1.0 - 2.0 * (yy + zz)) * sx,
        (2.0 * (xy + wz)) * sx,
        (2.0 * (xz - wy)) * sx,
        0.0,
        (2.0 * (xy - wz)) * sy,
        (1.0 - 2.0 * (xx + zz)) * sy,
        (2.0 * (yz + wx)) * sy,
        0.0,
        (2.0 * (xz + wy)) * sz,
        (2.0 * (yz - wx)) * sz,
        (1.0 - 2.0 * (xx + yy)) * sz,
        0.0,
        tx,
        ty,
        tz,
        1.0,
    ])
}

fn numeric_array(value: &Value) -> Option<Vec<f64>> {
    value.as_array()?.iter().map(Value::as_f64).collect()
}

fn accessor_index(attributes: &Map<String, Value>, name: &str) -> Result<usize, String> {
    attributes
        .get(name)
        .and_then(Value::as_u64)
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| format!("GLB mesh is missing the {name} accessor."))
}

type AccessorData = (usize, usize, usize, Vec<Vec<f64>>);

fn accessor_values(glb: &Glb, index: usize, label: &str) -> Result<AccessorData, String> {
    let accessors = glb.json["accessors"]
        .as_array()
        .ok_or_else(|| format!("{label} accessor table is missing."))?;
    let accessor = accessors
        .get(index)
        .ok_or_else(|| format!("{label} accessor index is invalid."))?;
    if accessor.get("sparse").is_some() {
        return Err(format!("Sparse {label} accessors are not supported."));
    }
    let count = accessor["count"]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| format!("{label} accessor count is invalid."))?;
    let component_type = accessor["componentType"]
        .as_u64()
        .ok_or_else(|| format!("{label} component type is invalid."))?
        as usize;
    let accessor_type = accessor["type"]
        .as_str()
        .ok_or_else(|| format!("{label} data type is invalid."))?;
    let components = type_components(accessor_type)?;
    let component_size = match component_type {
        5120 | 5121 => 1,
        5122 | 5123 => 2,
        5125 | 5126 => 4,
        _ => return Err(format!("{label} component type is unsupported.")),
    };
    let accessor_view = accessor["bufferView"]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| format!("{label} buffer view is missing."))?;
    let views = glb.json["bufferViews"]
        .as_array()
        .ok_or_else(|| format!("{label} buffer view table is missing."))?;
    let view = views
        .get(accessor_view)
        .ok_or_else(|| format!("{label} buffer view index is invalid."))?;
    if view.get("buffer").and_then(Value::as_u64).unwrap_or(0) != 0 {
        return Err(format!("{label} references a non-binary buffer."));
    }
    let view_start = view.get("byteOffset").and_then(Value::as_u64).unwrap_or(0) as usize;
    let view_len = view["byteLength"]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| format!("{label} view length is invalid."))?;
    let declared_len = glb.json["buffers"][0]["byteLength"]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| format!("{label} buffer length is invalid."))?;
    if view_start
        .checked_add(view_len)
        .is_none_or(|end| end > declared_len)
    {
        return Err(format!(
            "{label} buffer view exceeds the declared binary buffer."
        ));
    }
    let relative = accessor
        .get("byteOffset")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let element_size = component_size * components;
    let stride = view
        .get("byteStride")
        .and_then(Value::as_u64)
        .map(|n| n as usize)
        .unwrap_or(element_size);
    if stride < element_size {
        return Err(format!("{label} byte stride is invalid."));
    }
    let relative_end = if count == 0 {
        relative
    } else {
        relative
            .checked_add(
                stride
                    .checked_mul(count - 1)
                    .ok_or_else(|| format!("{label} size overflow."))?,
            )
            .and_then(|n| n.checked_add(element_size))
            .ok_or_else(|| format!("{label} size overflow."))?
    };
    if relative_end > view_len {
        return Err(format!("{label} data exceeds its buffer view."));
    }
    let start = view_start
        .checked_add(relative)
        .ok_or_else(|| format!("{label} offset overflow."))?;
    let mut rows = Vec::with_capacity(count);
    for row in 0..count {
        let base = start + row * stride;
        let mut values = Vec::with_capacity(components);
        for component in 0..components {
            let at = base + component * component_size;
            values.push(read_component(
                &glb.bin,
                at,
                component_type,
                accessor
                    .get("normalized")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            )?);
        }
        rows.push(values);
    }
    Ok((count, component_type, components, rows))
}

fn type_components(kind: &str) -> Result<usize, String> {
    match kind {
        "SCALAR" => Ok(1),
        "VEC2" => Ok(2),
        "VEC3" => Ok(3),
        "VEC4" => Ok(4),
        "MAT4" => Ok(16),
        _ => Err(format!("Unsupported glTF accessor type {kind}.")),
    }
}

fn read_component(
    bytes: &[u8],
    offset: usize,
    kind: usize,
    normalized: bool,
) -> Result<f64, String> {
    let result = match kind {
        5120 => {
            let value = *bytes.get(offset).ok_or("GLB accessor is truncated")? as i8;
            if normalized {
                (value as f64 / 127.0).max(-1.0)
            } else {
                value as f64
            }
        }
        5121 => {
            let value = *bytes.get(offset).ok_or("GLB accessor is truncated")?;
            if normalized {
                value as f64 / 255.0
            } else {
                value as f64
            }
        }
        5122 => {
            let value = i16::from_le_bytes(
                bytes
                    .get(offset..offset + 2)
                    .ok_or("GLB accessor is truncated")?
                    .try_into()
                    .map_err(|_| "GLB accessor is truncated")?,
            );
            if normalized {
                (value as f64 / 32767.0).max(-1.0)
            } else {
                value as f64
            }
        }
        5123 => {
            let value = u16::from_le_bytes(
                bytes
                    .get(offset..offset + 2)
                    .ok_or("GLB accessor is truncated")?
                    .try_into()
                    .map_err(|_| "GLB accessor is truncated")?,
            );
            if normalized {
                value as f64 / 65535.0
            } else {
                value as f64
            }
        }
        5125 => u32::from_le_bytes(
            bytes
                .get(offset..offset + 4)
                .ok_or("GLB accessor is truncated")?
                .try_into()
                .map_err(|_| "GLB accessor is truncated")?,
        ) as f64,
        5126 => f32::from_le_bytes(
            bytes
                .get(offset..offset + 4)
                .ok_or("GLB accessor is truncated")?
                .try_into()
                .map_err(|_| "GLB accessor is truncated")?,
        ) as f64,
        _ => return Err("Unsupported glTF component type.".into()),
    };
    if !result.is_finite() {
        return Err("GLB accessor contains a non-finite value.".into());
    }
    Ok(result)
}

fn read_float_accessor(glb: &Glb, index: usize, label: &str) -> Result<Vec<[u32; 3]>, String> {
    let accessor = glb.json["accessors"]
        .get(index)
        .ok_or_else(|| format!("{label} accessor is missing."))?;
    if accessor["type"].as_str() != Some("VEC3") || accessor["componentType"].as_u64() != Some(5126)
    {
        return Err(format!("{label} must contain float32 VEC3 positions."));
    }
    let (count, _, _, values) = accessor_values(glb, index, label)?;
    if values.len() != count {
        return Err(format!("{label} count is invalid."));
    }
    values
        .into_iter()
        .map(|row| {
            let mut out = [0u32; 3];
            for (index, value) in row.into_iter().enumerate() {
                out[index] = (value as f32).to_bits();
            }
            Ok(out)
        })
        .collect()
}

fn read_indices(glb: &Glb, primitive: &Value, vertex_count: usize) -> Result<Vec<u32>, String> {
    let Some(index) = primitive.get("indices").and_then(Value::as_u64) else {
        return Ok((0..vertex_count).map(|n| n as u32).collect());
    };
    let index = usize::try_from(index).map_err(|_| "Invalid index accessor.")?;
    let accessor = glb.json["accessors"]
        .get(index)
        .ok_or_else(|| "GLB index accessor is missing.".to_string())?;
    if accessor["type"].as_str() != Some("SCALAR") {
        return Err("GLB indices must be SCALAR.".into());
    }
    let (count, component_type, _, values) = accessor_values(glb, index, "indices")?;
    if !matches!(component_type, 5121 | 5123 | 5125) {
        return Err("GLB indices must use an unsigned integer type.".into());
    }
    if values.len() != count {
        return Err("GLB index count is invalid.".into());
    }
    values
        .into_iter()
        .map(|row| {
            let value = row.first().copied().unwrap_or(-1.0);
            if value < 0.0 || value as usize >= vertex_count {
                return Err("GLB index points outside its vertex array.".into());
            }
            Ok(value as u32)
        })
        .collect()
}

fn read_unsigned_accessor(glb: &Glb, index: usize, label: &str) -> Result<Vec<Vec<u32>>, String> {
    let accessor = glb.json["accessors"]
        .get(index)
        .ok_or_else(|| format!("{label} accessor is missing."))?;
    if accessor["type"].as_str() != Some("VEC4")
        || !matches!(accessor["componentType"].as_u64(), Some(5121 | 5123))
    {
        return Err(format!(
            "{label} must contain unsigned-byte or unsigned-short VEC4 values."
        ));
    }
    if accessor.get("normalized").and_then(Value::as_bool) == Some(true) {
        return Err(format!("{label} cannot use normalized values."));
    }
    let (_, _, _, rows) = accessor_values(glb, index, label)?;
    rows.into_iter()
        .map(|row| {
            row.into_iter()
                .map(|v| u32::try_from(v as u64).map_err(|_| format!("{label} value is invalid.")))
                .collect()
        })
        .collect()
}

fn read_weight_accessor(glb: &Glb, index: usize) -> Result<Vec<Vec<f64>>, String> {
    let accessor = glb.json["accessors"]
        .get(index)
        .ok_or_else(|| "SkinTokens WEIGHTS_0 accessor is missing.".to_string())?;
    if accessor["type"].as_str() != Some("VEC4")
        || !matches!(accessor["componentType"].as_u64(), Some(5121 | 5123 | 5126))
    {
        return Err(
            "SkinTokens WEIGHTS_0 must contain normalized integer or float32 VEC4 values.".into(),
        );
    }
    if accessor["componentType"].as_u64() != Some(5126)
        && accessor.get("normalized").and_then(Value::as_bool) != Some(true)
    {
        return Err("Integer SkinTokens weights must be normalized.".into());
    }
    let (_, _, _, values) = accessor_values(glb, index, "SkinTokens WEIGHTS_0")?;
    if values.iter().flatten().any(|weight| *weight < 0.0) {
        return Err("SkinTokens produced negative skin weights.".into());
    }
    Ok(values)
}

fn skeleton_node_closure(nodes: &[Value], skin: &Value) -> Result<Vec<usize>, String> {
    let joints = skin["joints"]
        .as_array()
        .ok_or_else(|| "SkinTokens skin has no joint list.".to_string())?;
    let mut parent = HashMap::new();
    for (index, node) in nodes.iter().enumerate() {
        if let Some(children) = node.get("children").and_then(Value::as_array) {
            for child in children {
                let child = child
                    .as_u64()
                    .and_then(|n| usize::try_from(n).ok())
                    .ok_or_else(|| "SkinTokens joint child index is invalid.".to_string())?;
                if child >= nodes.len() || parent.insert(child, index).is_some() {
                    return Err("SkinTokens skeleton hierarchy is invalid.".into());
                }
            }
        }
    }
    let mut include = HashSet::new();
    for joint in joints.iter().chain(skin.get("skeleton")) {
        let mut current = joint
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .ok_or_else(|| "SkinTokens joint index is invalid.".to_string())?;
        let mut chain = HashSet::new();
        loop {
            if current >= nodes.len() {
                return Err("SkinTokens joint index is outside its node table.".into());
            }
            if !chain.insert(current) {
                return Err("SkinTokens skeleton hierarchy contains a cycle.".into());
            }
            include.insert(current);
            match parent.get(&current).copied() {
                Some(next) => current = next,
                None => break,
            }
        }
    }
    let mut included: Vec<usize> = include.into_iter().collect();
    included.sort_unstable();
    Ok(included)
}

fn copy_accessor(
    source: &mut Glb,
    rig: &Glb,
    index: usize,
    views: &mut HashMap<usize, usize>,
) -> Result<usize, String> {
    let rig_accessor = rig.json["accessors"]
        .as_array()
        .and_then(|items| items.get(index))
        .ok_or_else(|| "SkinTokens accessor index is invalid.".to_string())?;
    if rig_accessor.get("sparse").is_some() {
        return Err("Sparse SkinTokens accessors are not supported.".into());
    }
    let old_view = rig_accessor["bufferView"]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| "SkinTokens accessor has no buffer view.".to_string())?;
    let new_view = if let Some(mapped) = views.get(&old_view) {
        *mapped
    } else {
        let source_views = source.json["bufferViews"]
            .as_array()
            .ok_or_else(|| "Source GLB has no buffer view array.".to_string())?;
        let rig_view = rig.json["bufferViews"]
            .as_array()
            .and_then(|items| items.get(old_view))
            .ok_or_else(|| "SkinTokens buffer view is invalid.".to_string())?;
        if rig_view.get("buffer").and_then(Value::as_u64).unwrap_or(0) != 0 {
            return Err("SkinTokens accessor references a non-binary buffer.".into());
        }
        let offset = rig_view
            .get("byteOffset")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        let length = rig_view["byteLength"]
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .ok_or_else(|| "SkinTokens buffer view size is invalid.".to_string())?;
        let declared_len = rig.json["buffers"][0]["byteLength"]
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .ok_or_else(|| "SkinTokens buffer length is invalid.".to_string())?;
        if offset
            .checked_add(length)
            .is_none_or(|end| end > declared_len)
        {
            return Err("SkinTokens buffer view exceeds the declared binary buffer.".into());
        }
        let data = rig
            .bin
            .get(
                offset
                    ..offset
                        .checked_add(length)
                        .ok_or_else(|| "SkinTokens buffer range overflow.".to_string())?,
            )
            .ok_or_else(|| "SkinTokens buffer view exceeds its binary chunk.".to_string())?;
        while !source.bin.len().is_multiple_of(4) {
            source.bin.push(0);
        }
        let new_offset = source.bin.len();
        source.bin.extend_from_slice(data);
        let mut view = rig_view.clone();
        view["buffer"] = Value::from(0);
        view["byteOffset"] = Value::from(new_offset as u64);
        let new_index = source_views.len();
        source.json["bufferViews"]
            .as_array_mut()
            .expect("checked source views")
            .push(view);
        views.insert(old_view, new_index);
        new_index
    };
    let mut accessor = rig_accessor.clone();
    accessor["bufferView"] = Value::from(new_view as u64);
    let accessors = source.json["accessors"]
        .as_array_mut()
        .ok_or_else(|| "Source GLB has no accessor array.".to_string())?;
    let new_index = accessors.len();
    accessors.push(accessor);
    Ok(new_index)
}
