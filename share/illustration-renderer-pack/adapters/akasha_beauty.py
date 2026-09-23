#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Akasha Illustration Renderer Pack — minimal beauty adapter.

Runs ONLY inside Blender (`blender -b --python akasha_beauty.py -- <work_dir>`).
Reads `scene.json` (Akasha Y-up ADR 0011 export) and writes `beauty.png`.

This file must NOT be vendored into Apache guest modules or AGPL host crates.
"""

from __future__ import annotations

import json
import math
import os
import sys
from typing import Any, Dict, List, Optional, Tuple

# Akasha Y-up RH → Blender Z-up: (x, y, z) → (x, -z, y) ≡ rotate +90° about X.
_HALF = math.sqrt(0.5)
# Quaternion xyzw for +90° about X (left-multiply onto Akasha world rotation).
Q_BASIS_XYZW = (_HALF, 0.0, 0.0, _HALF)


def qmul_xyzw(
    a: Tuple[float, float, float, float], b: Tuple[float, float, float, float]
) -> Tuple[float, float, float, float]:
    ax, ay, az, aw = a
    bx, by, bz, bw = b
    return (
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    )


def qrot_xyzw(
    q: Tuple[float, float, float, float], v: Tuple[float, float, float]
) -> Tuple[float, float, float]:
    """Rotate vector `v` by unit quaternion `q` (xyzw)."""
    x, y, z, w = q
    vx, vy, vz = v
    tx = 2.0 * (y * vz - z * vy)
    ty = 2.0 * (z * vx - x * vz)
    tz = 2.0 * (x * vy - y * vx)
    return (
        vx + w * tx + (y * tz - z * ty),
        vy + w * ty + (z * tx - x * tz),
        vz + w * tz + (x * ty - y * tx),
    )


def y_up_vec_to_blender(v: Tuple[float, float, float]) -> Tuple[float, float, float]:
    x, y, z = v
    return (x, -z, y)


def y_up_quat_to_blender(
    r_xyzw: Tuple[float, float, float, float],
) -> Tuple[float, float, float, float]:
    """Bake Y-up→Z-up into orientation: q_b = q_basis * q_akasha.

    Component-shuffle / conjugation alone leaves identity cameras looking along
    Blender −Z after position remap, which misses the remapped scene (Akasha −Z
    becomes Blender +Y). Left-multiply by q_basis aims local −Z correctly.
    """
    return qmul_xyzw(Q_BASIS_XYZW, r_xyzw)


def y_up_to_blender(t, r_xyzw, s):
    """Convert Akasha Y-up RH translation/quat/scale into Blender Z-up.

    Position: (x, y, z)_akasha → (x, -z, y)_blender.
    Rotation: q_blender = q_basis(+90° X) * q_akasha (xyzw).
    Scale: keep (sx, sy, sz) in object-local space — q_basis already remaps
    local axes into Blender world. Permuting scale *and* applying q_basis
    double-converts non-uniform boxes (ground thin-Y becomes a vertical wall).
    """
    tx, ty, tz = t
    sx, sy, sz = s
    qx, qy, qz, qw = r_xyzw
    pos = y_up_vec_to_blender((float(tx), float(ty), float(tz)))
    scale = (float(sx), float(sy), float(sz))
    quat = y_up_quat_to_blender((float(qx), float(qy), float(qz), float(qw)))
    return pos, quat, scale


def _as_vec3(v: Any, default: Tuple[float, float, float]) -> Tuple[float, float, float]:
    if not isinstance(v, (list, tuple)) or len(v) < 3:
        return default
    return (float(v[0]), float(v[1]), float(v[2]))


def _as_quat(v: Any) -> Tuple[float, float, float, float]:
    if not isinstance(v, (list, tuple)) or len(v) < 4:
        return (0.0, 0.0, 0.0, 1.0)
    return (float(v[0]), float(v[1]), float(v[2]), float(v[3]))


def _compose_local(
    parent: Optional[Tuple[Tuple[float, float, float], Tuple[float, float, float, float], Tuple[float, float, float]]],
    local_t: Tuple[float, float, float],
    local_q: Tuple[float, float, float, float],
    local_s: Tuple[float, float, float],
) -> Tuple[Tuple[float, float, float], Tuple[float, float, float, float], Tuple[float, float, float]]:
    """Compose Akasha parent world * local TRS (uniform-scale-friendly)."""
    if parent is None:
        return local_t, local_q, local_s
    pt, pq, ps = parent
    # world_t = parent_t + rotate(parent_q, parent_s * local_t)
    scaled = (ps[0] * local_t[0], ps[1] * local_t[1], ps[2] * local_t[2])
    rotated = qrot_xyzw(pq, scaled)
    wt = (pt[0] + rotated[0], pt[1] + rotated[1], pt[2] + rotated[2])
    wq = qmul_xyzw(pq, local_q)
    ws = (ps[0] * local_s[0], ps[1] * local_s[1], ps[2] * local_s[2])
    return wt, wq, ws


def compute_akasha_world_trs(
    nodes: Dict[str, Any],
) -> Dict[str, Tuple[Tuple[float, float, float], Tuple[float, float, float, float], Tuple[float, float, float]]]:
    """World TRS in Akasha Y-up for every exported node (parent chain)."""
    cache: Dict[
        str, Tuple[Tuple[float, float, float], Tuple[float, float, float, float], Tuple[float, float, float]]
    ] = {}

    def world_of(node_id: str, stack: List[str]):
        if node_id in cache:
            return cache[node_id]
        if node_id in stack:
            # Cycle — treat as local only.
            node = nodes.get(node_id) or {}
            t = _as_vec3(node.get("translation"), (0.0, 0.0, 0.0))
            q = _as_quat(node.get("rotation_xyzw"))
            s = _as_vec3(node.get("scale"), (1.0, 1.0, 1.0))
            cache[node_id] = (t, q, s)
            return cache[node_id]
        node = nodes.get(node_id) or {}
        t = _as_vec3(node.get("translation"), (0.0, 0.0, 0.0))
        q = _as_quat(node.get("rotation_xyzw"))
        s = _as_vec3(node.get("scale"), (1.0, 1.0, 1.0))
        parent_id = node.get("parent")
        parent_world = None
        if parent_id and parent_id in nodes:
            parent_world = world_of(str(parent_id), stack + [node_id])
        cache[node_id] = _compose_local(parent_world, t, q, s)
        return cache[node_id]

    for nid in nodes:
        world_of(nid, [])
    return cache


def mesh_centroid_akasha(nodes: Dict[str, Any], world: Dict[str, Any]) -> Tuple[float, float, float]:
    pts: List[Tuple[float, float, float]] = []
    for nid, node in nodes.items():
        if not node.get("visible", True):
            continue
        if node.get("kind") not in ("mesh_box", "mesh_asset"):
            continue
        if nid in world:
            pts.append(world[nid][0])
    if not pts:
        return (0.0, 0.5, 0.0)
    sx = sum(p[0] for p in pts) / len(pts)
    sy = sum(p[1] for p in pts) / len(pts)
    sz = sum(p[2] for p in pts) / len(pts)
    return (sx, sy, sz)


def look_at_quat_blender(
    eye: Tuple[float, float, float],
    target: Tuple[float, float, float],
    up: Tuple[float, float, float] = (0.0, 0.0, 1.0),
) -> Tuple[float, float, float, float]:
    """Blender camera quaternion (xyzw) so local −Z aims at target (Z-up world)."""
    # Forward (look) = normalize(target - eye); camera looks down local −Z.
    fx = target[0] - eye[0]
    fy = target[1] - eye[1]
    fz = target[2] - eye[2]
    fl = math.sqrt(fx * fx + fy * fy + fz * fz) or 1.0
    fx, fy, fz = fx / fl, fy / fl, fz / fl
    # Right = forward × up
    rx = fy * up[2] - fz * up[1]
    ry = fz * up[0] - fx * up[2]
    rz = fx * up[1] - fy * up[0]
    rl = math.sqrt(rx * rx + ry * ry + rz * rz)
    if rl < 1e-6:
        up = (0.0, 1.0, 0.0)
        rx = fy * up[2] - fz * up[1]
        ry = fz * up[0] - fx * up[2]
        rz = fx * up[1] - fy * up[0]
        rl = math.sqrt(rx * rx + ry * ry + rz * rz) or 1.0
    rx, ry, rz = rx / rl, ry / rl, rz / rl
    # True up = right × forward
    ux = ry * fz - rz * fy
    uy = rz * fx - rx * fz
    uz = rx * fy - ry * fx
    # Rotation matrix columns = right, up, -forward (camera local axes in world)
    # Local −Z = forward ⇒ third column = −forward
    r00, r01, r02 = rx, ux, -fx
    r10, r11, r12 = ry, uy, -fy
    r20, r21, r22 = rz, uz, -fz
    # Matrix → quaternion (xyzw), Shepperd
    trace = r00 + r11 + r22
    if trace > 0.0:
        s = math.sqrt(trace + 1.0) * 2.0
        qw = 0.25 * s
        qx = (r21 - r12) / s
        qy = (r02 - r20) / s
        qz = (r10 - r01) / s
    elif r00 > r11 and r00 > r22:
        s = math.sqrt(1.0 + r00 - r11 - r22) * 2.0
        qw = (r21 - r12) / s
        qx = 0.25 * s
        qy = (r01 + r10) / s
        qz = (r02 + r20) / s
    elif r11 > r22:
        s = math.sqrt(1.0 + r11 - r00 - r22) * 2.0
        qw = (r02 - r20) / s
        qx = (r01 + r10) / s
        qy = 0.25 * s
        qz = (r12 + r21) / s
    else:
        s = math.sqrt(1.0 + r22 - r00 - r11) * 2.0
        qw = (r10 - r01) / s
        qx = (r02 + r20) / s
        qy = (r12 + r21) / s
        qz = 0.25 * s
    return (qx, qy, qz, qw)


def _argv_work_dir() -> str:
    if "--" in sys.argv:
        i = sys.argv.index("--")
        if i + 1 < len(sys.argv):
            return sys.argv[i + 1]
    return os.getcwd()


def apply_material_override(obj, override, bpy):
    """Copy imported PBR materials before changing this scene instance."""
    if not override or obj.type != "MESH":
        return
    tint = override.get("tint")
    roughness = override.get("roughness")
    metallic = override.get("metallic")
    if not obj.material_slots:
        material = bpy.data.materials.new(name=f"override_{obj.name}")
        material.use_nodes = True
        obj.data.materials.append(material)
    for slot in obj.material_slots:
        original = slot.material
        if original is None:
            continue
        material = original.copy()
        material.use_nodes = True
        slot.material = material
        bsdf = next((node for node in material.node_tree.nodes if node.type == "BSDF_PRINCIPLED"), None)
        if bsdf is None:
            continue
        if tint is not None:
            rgb = tuple(float(v) for v in tint)
            color = bsdf.inputs["Base Color"]
            incoming = next(iter(color.links), None)
            if incoming is not None:
                source = incoming.from_socket
                material.node_tree.links.remove(incoming)
                multiply = material.node_tree.nodes.new("ShaderNodeMixRGB")
                multiply.blend_type = "MULTIPLY"
                multiply.inputs[0].default_value = 1.0
                multiply.inputs[2].default_value = (*rgb, 1.0)
                material.node_tree.links.new(source, multiply.inputs[1])
                material.node_tree.links.new(multiply.outputs[0], color)
            else:
                base = color.default_value
                color.default_value = (base[0] * rgb[0], base[1] * rgb[1], base[2] * rgb[2], base[3])
        for input_name, value in (("Roughness", roughness), ("Metallic", metallic)):
            if value is None:
                continue
            socket = bsdf.inputs[input_name]
            for link in list(socket.links):
                material.node_tree.links.remove(link)
            socket.default_value = float(value)


def main() -> int:
    import bpy  # type: ignore  # only available inside Blender

    work = _argv_work_dir()
    scene_path = os.path.join(work, "scene.json")
    out_path = os.path.join(work, "beauty.png")
    with open(scene_path, "r", encoding="utf-8") as f:
        data = json.load(f)

    # Fresh empty scene
    bpy.ops.wm.read_factory_settings(use_empty=True)
    # Remove default objects if any remain
    for obj in list(bpy.data.objects):
        bpy.data.objects.remove(obj, do_unlink=True)

    width = int(data.get("width") or 256)
    height = int(data.get("height") or 256)
    nodes = data.get("nodes") or {}
    blender_objs = {}

    # World-space placement avoids broken hierarchy after basis change: convert
    # composed Akasha world TRS once, do not re-parent (parent links are baked).
    world_akasha = compute_akasha_world_trs(nodes)
    target_akasha = mesh_centroid_akasha(nodes, world_akasha)
    target_blender = y_up_vec_to_blender(target_akasha)

    for node_id, node in nodes.items():
        if not node.get("visible", True):
            continue
        kind = node.get("kind")
        wt, wq, ws = world_akasha.get(
            node_id,
            (
                _as_vec3(node.get("translation"), (0.0, 0.0, 0.0)),
                _as_quat(node.get("rotation_xyzw")),
                _as_vec3(node.get("scale"), (1.0, 1.0, 1.0)),
            ),
        )
        pos, quat, scale = y_up_to_blender(wt, wq, ws)
        if kind == "mesh_box":
            bpy.ops.mesh.primitive_cube_add(size=1.0, location=pos)
            obj = bpy.context.active_object
            obj.name = node.get("name") or node_id
            obj.scale = scale
            obj.rotation_mode = "QUATERNION"
            obj.rotation_quaternion = (quat[3], quat[0], quat[1], quat[2])  # wxyz
            # Readable default matte so beauty is not paper-only when Freestyle is off.
            mat = bpy.data.materials.new(name=f"mat_{node_id}")
            mat.use_nodes = True
            bsdf = mat.node_tree.nodes.get("Principled BSDF")
            if bsdf is not None:
                bsdf.inputs["Base Color"].default_value = (0.22, 0.24, 0.28, 1.0)
                bsdf.inputs["Roughness"].default_value = 0.65
            if obj.data.materials:
                obj.data.materials[0] = mat
            else:
                obj.data.materials.append(mat)
            apply_material_override(obj, node.get("material"), bpy)
            blender_objs[node_id] = obj
        elif kind == "mesh_asset":
            # The host copies validated GLBs into this job's assets directory.
            # Blender's glTF importer keeps the file's hierarchy and PBR nodes.
            from mathutils import Matrix, Quaternion  # type: ignore

            relative = str(node.get("mesh_uri") or "")
            asset_path = os.path.realpath(os.path.join(work, relative))
            work_path = os.path.realpath(work)
            if not relative.lower().endswith((".glb", ".gltf")) or os.path.commonpath((work_path, asset_path)) != work_path:
                raise ValueError(f"invalid staged asset path for {node_id}")
            if not os.path.isfile(asset_path):
                raise FileNotFoundError(f"staged asset missing for {node_id}: {relative}")

            before = set(bpy.data.objects)
            bpy.ops.import_scene.gltf(filepath=asset_path)
            imported = set(bpy.data.objects) - before
            if not imported:
                raise ValueError(f"GLB has no Blender objects: {relative}")

            basis = Matrix.Rotation(math.pi / 2.0, 4, "X")
            local = (
                Matrix.Translation(wt)
                @ Quaternion((wq[3], wq[0], wq[1], wq[2])).to_matrix().to_4x4()
                @ Matrix.Diagonal((ws[0], ws[1], ws[2], 1.0))
            )
            anchor = bpy.data.objects.new(node.get("name") or node_id, None)
            bpy.context.scene.collection.objects.link(anchor)
            anchor.matrix_world = basis @ local @ basis.inverted()
            for imported_root in (obj for obj in imported if obj.parent not in imported):
                original_world = imported_root.matrix_world.copy()
                imported_root.parent = anchor
                imported_root.matrix_world = anchor.matrix_world @ original_world
            for imported_obj in imported:
                apply_material_override(imported_obj, node.get("material"), bpy)
            blender_objs[node_id] = anchor
        elif kind == "camera":
            cam_data = bpy.data.cameras.new(name=node.get("name") or node_id)
            cam_obj = bpy.data.objects.new(cam_data.name, cam_data)
            bpy.context.scene.collection.objects.link(cam_obj)
            cam_obj.location = pos
            # Prefer look-at mesh centroid (parity with host CPU beauty), falling
            # back to converted world quaternion when target coincides with eye.
            dx = target_blender[0] - pos[0]
            dy = target_blender[1] - pos[1]
            dz = target_blender[2] - pos[2]
            if (dx * dx + dy * dy + dz * dz) > 1e-8:
                quat = look_at_quat_blender(pos, target_blender)
            cam_obj.rotation_mode = "QUATERNION"
            cam_obj.rotation_quaternion = (quat[3], quat[0], quat[1], quat[2])
            cam = node.get("camera") or {}
            cam_data.lens = float(cam.get("focal_mm") or 50.0)
            cam_data.sensor_width = float(cam.get("sensor_width_mm") or 36.0)
            cam_data.clip_start = float(cam.get("near") or 0.1)
            cam_data.clip_end = float(cam.get("far") or 100.0)
            blender_objs[node_id] = cam_obj
        elif kind == "light":
            light_meta = node.get("light") or {}
            ltype = str(light_meta.get("type") or "point").lower()
            blender_type = {
                "point": "POINT",
                "directional": "SUN",
                "spot": "SPOT",
                "area": "AREA",
            }.get(ltype, "POINT")
            light_data = bpy.data.lights.new(name=node.get("name") or node_id, type=blender_type)
            # Intensity is unitless linear in Akasha; map to Blender energy heuristically.
            intensity = float(light_meta.get("intensity") or 1.5)
            if blender_type == "SUN":
                light_data.energy = max(0.05, intensity * 2.0)
            else:
                light_data.energy = max(1.0, intensity * 120.0)
            color = light_meta.get("color") or [1.0, 0.95, 0.88]
            try:
                light_data.color = (float(color[0]), float(color[1]), float(color[2]))
            except (TypeError, ValueError, IndexError):
                light_data.color = (1.0, 0.95, 0.88)
            if blender_type == "SPOT":
                spot_angle = float(light_meta.get("spot_angle_rad") or 0.785398)
                light_data.spot_size = max(0.05, min(3.14, spot_angle * 2.0))
            light_obj = bpy.data.objects.new(light_data.name, light_data)
            bpy.context.scene.collection.objects.link(light_obj)
            light_obj.location = pos
            light_obj.rotation_mode = "QUATERNION"
            light_obj.rotation_quaternion = (quat[3], quat[0], quat[1], quat[2])
            blender_objs[node_id] = light_obj
        else:
            empty = bpy.data.objects.new(node.get("name") or node_id, None)
            bpy.context.scene.collection.objects.link(empty)
            empty.location = pos
            empty.rotation_mode = "QUATERNION"
            empty.rotation_quaternion = (quat[3], quat[0], quat[1], quat[2])
            empty.scale = scale
            blender_objs[node_id] = empty

    # Imported GLBs may contain their own offsets and nested transforms. Frame
    # their actual world-space bounds instead of only the SceneGraph anchor.
    from mathutils import Vector  # type: ignore

    mesh_points = [obj.matrix_world @ Vector(corner)
                   for obj in bpy.data.objects if obj.type == "MESH"
                   for corner in obj.bound_box]
    if mesh_points:
        target_blender = tuple(
            (min(p[axis] for p in mesh_points) + max(p[axis] for p in mesh_points)) * 0.5
            for axis in range(3)
        )
        for obj in blender_objs.values():
            if obj.type == "CAMERA":
                eye = tuple(obj.location)
                q = look_at_quat_blender(eye, target_blender)
                obj.rotation_quaternion = (q[3], q[0], q[1], q[2])

    scene = bpy.context.scene
    active = data.get("active_camera")
    if active and active in blender_objs:
        scene.camera = blender_objs[active]
    elif blender_objs:
        # fallback first camera
        for n in blender_objs.values():
            if n.type == "CAMERA":
                scene.camera = n
                break

    if scene.camera is None:
        bpy.ops.object.camera_add(location=(0.0, -6.5, 2.2))
        scene.camera = bpy.context.active_object
        scene.camera.rotation_mode = "QUATERNION"
        la = look_at_quat_blender(
            (scene.camera.location.x, scene.camera.location.y, scene.camera.location.z),
            target_blender,
        )
        scene.camera.rotation_quaternion = (la[3], la[0], la[1], la[2])

    # Default area light if none
    if not any(o.type == "LIGHT" for o in bpy.data.objects):
        light_data = bpy.data.lights.new(name="AosKey", type="AREA")
        light_data.energy = 400.0
        light_obj = bpy.data.objects.new("AosKey", light_data)
        bpy.context.scene.collection.objects.link(light_obj)
        light_obj.location = (3.0, -3.0, 5.0)

    scene.render.engine = "CYCLES"
    scene.cycles.device = "CPU"
    scene.cycles.samples = 16
    # Distro Blender builds often omit OpenImageDenoiser; leave denoise off
    # so beauty stays fail-closed on geometry, not on optional denoise deps.
    if hasattr(scene.cycles, "use_denoising"):
        scene.cycles.use_denoising = False
    view_layer = bpy.context.view_layer
    if hasattr(view_layer, "cycles") and hasattr(view_layer.cycles, "use_denoising"):
        view_layer.cycles.use_denoising = False
    scene.render.resolution_x = width
    scene.render.resolution_y = height
    scene.render.filepath = out_path
    scene.render.image_settings.file_format = "PNG"

    # Optional NPR style from Akasha export (Sketch / Pencil / Ink).
    # Freestyle line art is the pack-side approximation; host stays bpy-free.
    style = data.get("style") or {}
    family = str(style.get("family") or "").lower()
    if family in ("sketch", "pencil", "ink"):
        scene.render.use_freestyle = True
        scene.cycles.samples = 32 if style.get("antialias", True) else 8
        try:
            fs = bpy.context.view_layer.freestyle_settings
            if fs.linesets:
                lineset = fs.linesets[0]
            else:
                lineset = fs.linesets.new("AosLineSet")
            # Factory-empty scenes ship a lineset with linestyle=None; create one.
            if lineset.linestyle is None:
                lineset.linestyle = bpy.data.linestyles.new("AosLineStyle")
            linestyle = lineset.linestyle
            linewidth = float(style.get("line_width") or 1.2)
            linestyle.thickness = max(0.3, min(linewidth, 4.0))
            density = max(0.0, min(float(style.get("line_density", 1.0)), 1.0))
            lineset.select_silhouette = True
            lineset.select_border = True
            lineset.select_external_contour = True
            lineset.select_crease = density >= 0.25
            lineset.select_material_boundary = density >= 0.6
            lineset.select_suggestive_contour = density >= 0.85
            variation = max(0.0, min(float(style.get("variation", 0.0)), 1.0))
            if variation > 0 and hasattr(linestyle, "geometry_modifiers"):
                noise = linestyle.geometry_modifiers.get("AkashaStrokeVariation")
                if noise is None:
                    noise = linestyle.geometry_modifiers.new("AkashaStrokeVariation", "SPATIAL_NOISE")
                noise.amplitude = variation * 2.0
                noise.scale = 20.0
            stroke = style.get("stroke_rgb") or [20, 18, 16]
            if hasattr(linestyle, "color"):
                linestyle.color = (
                    float(stroke[0]) / 255.0,
                    float(stroke[1]) / 255.0,
                    float(stroke[2]) / 255.0,
                )
            # Soften world for paper-like look when tint present.
            # `read_factory_settings(use_empty=True)` leaves `scene.world is None`
            # — without creating a World, empty-frustum beauty is pure black (or
            # reads as a blank square in DeclUI) instead of paper tint.
            tint = style.get("paper_tint") or [248, 246, 240]
            if getattr(scene, "world", None) is None:
                scene.world = bpy.data.worlds.new("AosPaperWorld")
            if scene.world is not None:
                scene.world.use_nodes = True
                bg = scene.world.node_tree.nodes.get("Background")
                if bg is not None:
                    bg.inputs[0].default_value = (
                        float(tint[0]) / 255.0,
                        float(tint[1]) / 255.0,
                        float(tint[2]) / 255.0,
                        1.0,
                    )
                    # Keep paper visible but not so bright it washes out Freestyle.
                    bg.inputs[1].default_value = 0.85
        except (AttributeError, TypeError, ValueError, RuntimeError) as exc:
            print(f"Akasha Freestyle configuration failed: {exc}")

    bpy.ops.render.render(write_still=True)
    # A fixed-image graphite hatch pass. It alters only the rendered image, not
    # imported PBR materials or the SceneGraph.
    hatch = max(0.0, min(float(style.get("hatching", 0.0)), 1.0))
    if family in ("sketch", "pencil", "ink") and hatch > 0:
        image = bpy.data.images.load(out_path, check_existing=False)
        pixels = list(image.pixels[:])
        stride = max(5, int(15 - hatch * 10))
        width_px, height_px = image.size
        background = pixels[:3]
        for y in range(height_px):
            for x in range(width_px):
                index = (y * width_px + x) * 4
                luminance = sum(pixels[index:index + 3]) / 3.0
                differs_from_paper = max(
                    abs(pixels[index + channel] - background[channel])
                    for channel in range(3)
                ) > 0.12
                if differs_from_paper and 0.15 < luminance < 0.85 and (x + y) % stride == 0:
                    factor = 1.0 - 0.35 * hatch
                    for channel in range(3):
                        pixels[index + channel] *= factor
        image.pixels[:] = pixels
        image.filepath_raw = out_path
        image.file_format = "PNG"
        image.save()
        bpy.data.images.remove(image)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
