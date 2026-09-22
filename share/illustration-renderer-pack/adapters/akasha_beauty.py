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


def _argv_work_dir() -> str:
    if "--" in sys.argv:
        i = sys.argv.index("--")
        if i + 1 < len(sys.argv):
            return sys.argv[i + 1]
    return os.getcwd()


def y_up_to_blender(t, r_xyzw, s):
    """Convert Akasha Y-up RH translation/quat/scale into Blender Z-up.

    Simple axis remap: (x, y, z)_akasha -> (x, -z, y)_blender for position.
    Quaternion: apply matching basis change (approx for MVP boxes).
    """
    tx, ty, tz = t
    sx, sy, sz = s
    qx, qy, qz, qw = r_xyzw
    pos = (tx, -tz, ty)
    scale = (sx, sz, sy)
    # Remap quaternion components for Y-up -> Z-up (rotate -90° about X).
    # q' = q_basis * q * q_basis^-1 with q_basis = rot_x(-90°)
    hx, hy, hz, hw = (math.sqrt(0.5), 0.0, 0.0, math.sqrt(0.5))  # +90 X for inverse path
    # For MVP: identity-ish remap of vector part
    quat = (qx, -qz, qy, qw)
    _ = (hx, hy, hz, hw)
    return pos, quat, scale


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

    for node_id, node in nodes.items():
        if not node.get("visible", True):
            continue
        kind = node.get("kind")
        pos, quat, scale = y_up_to_blender(
            node.get("translation") or [0, 0, 0],
            node.get("rotation_xyzw") or [0, 0, 0, 1],
            node.get("scale") or [1, 1, 1],
        )
        if kind == "mesh_box":
            bpy.ops.mesh.primitive_cube_add(size=1.0, location=pos)
            obj = bpy.context.active_object
            obj.name = node.get("name") or node_id
            obj.scale = scale
            obj.rotation_mode = "QUATERNION"
            obj.rotation_quaternion = (quat[3], quat[0], quat[1], quat[2])  # wxyz
            blender_objs[node_id] = obj
        elif kind == "camera":
            cam_data = bpy.data.cameras.new(name=node.get("name") or node_id)
            cam_obj = bpy.data.objects.new(cam_data.name, cam_data)
            bpy.context.scene.collection.objects.link(cam_obj)
            cam_obj.location = pos
            cam_obj.rotation_mode = "QUATERNION"
            cam_obj.rotation_quaternion = (quat[3], quat[0], quat[1], quat[2])
            cam = node.get("camera") or {}
            cam_data.lens = float(cam.get("focal_mm") or 50.0)
            cam_data.sensor_width = float(cam.get("sensor_width_mm") or 36.0)
            cam_data.clip_start = float(cam.get("near") or 0.1)
            cam_data.clip_end = float(cam.get("far") or 100.0)
            blender_objs[node_id] = cam_obj
        elif kind == "light":
            light_data = bpy.data.lights.new(name=node.get("name") or node_id, type="AREA")
            light_data.energy = 200.0
            light_obj = bpy.data.objects.new(light_data.name, light_data)
            bpy.context.scene.collection.objects.link(light_obj)
            light_obj.location = pos
            blender_objs[node_id] = light_obj
        else:
            empty = bpy.data.objects.new(node.get("name") or node_id, None)
            bpy.context.scene.collection.objects.link(empty)
            empty.location = pos
            blender_objs[node_id] = empty

    # Parent links (best-effort after all objects exist)
    for node_id, node in nodes.items():
        parent_id = node.get("parent")
        if parent_id and node_id in blender_objs and parent_id in blender_objs:
            child = blender_objs[node_id]
            parent = blender_objs[parent_id]
            child.parent = parent

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
        try:
            linestyle = bpy.context.view_layer.freestyle_settings.linesets[0].linestyle
            linewidth = float(style.get("line_width") or 1.2)
            linestyle.thickness = max(0.5, min(linewidth * 1.5, 6.0))
            jitter = float(style.get("jitter") or 0.0)
            if family == "sketch":
                linestyle.thickness *= 0.85
            elif family == "ink":
                linestyle.thickness *= 1.35
            # Soften world for paper-like look when tint present.
            tint = style.get("paper_tint") or [248, 246, 240]
            if hasattr(scene, "world") and scene.world is not None:
                scene.world.use_nodes = True
                bg = scene.world.node_tree.nodes.get("Background")
                if bg is not None:
                    bg.inputs[0].default_value = (
                        float(tint[0]) / 255.0,
                        float(tint[1]) / 255.0,
                        float(tint[2]) / 255.0,
                        1.0,
                    )
            _ = jitter  # reserved for future noise modifiers in pack scripts
        except Exception:
            # Freestyle / lineset may be unavailable in minimal builds — still render.
            pass

    bpy.ops.render.render(write_still=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
