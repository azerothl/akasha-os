#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Pure-Python regression tests for Y-up → Blender conversion (no bpy).

Run: python3 share/illustration-renderer-pack/adapters/test_akasha_beauty_math.py
"""

from __future__ import annotations

import math
import os
import sys
import unittest

# Allow `import akasha_beauty` when run from repo root or adapters/.
_HERE = os.path.dirname(os.path.abspath(__file__))
if _HERE not in sys.path:
    sys.path.insert(0, _HERE)

import akasha_beauty as ab  # noqa: E402


class TestYUpToBlender(unittest.TestCase):
    def test_identity_camera_looks_toward_remapped_forward(self):
        """Demo camera: Akasha (0, 2.2, 6.5) identity looks −Z → Blender +Y."""
        pos, quat, _scale = ab.y_up_to_blender(
            (0.0, 2.2, 6.5), (0.0, 0.0, 0.0, 1.0), (1.0, 1.0, 1.0)
        )
        self.assertAlmostEqual(pos[0], 0.0, places=5)
        self.assertAlmostEqual(pos[1], -6.5, places=5)
        self.assertAlmostEqual(pos[2], 2.2, places=5)
        look = ab.qrot_xyzw(quat, (0.0, 0.0, -1.0))
        # Must aim along +Y (toward origin), not −Z (empty frustum / white paper).
        self.assertAlmostEqual(look[0], 0.0, places=5)
        self.assertAlmostEqual(look[1], 1.0, places=5)
        self.assertAlmostEqual(look[2], 0.0, places=5)

    def test_old_component_shuffle_would_miss_scene(self):
        """Document the pre-fix bug: shuffle left identity looking −Z."""
        qx, qy, qz, qw = (0.0, 0.0, 0.0, 1.0)
        old = (qx, -qz, qy, qw)
        look_old = ab.qrot_xyzw(old, (0.0, 0.0, -1.0))
        self.assertAlmostEqual(look_old[2], -1.0, places=5)

    def test_ground_thin_y_stays_floor_not_wall(self):
        """Non-uniform ground: thin Akasha Y must stay thin Blender Z (floor).

        Permuting scale (sx,sz,sy) while also applying q_basis tipped floors
        into vertical walls and laid characters on their side.
        """
        # Unit-cube corners in local space, half-extent 0.5 (Akasha mesh_box).
        local = [
            (-0.5, -0.5, -0.5),
            (0.5, -0.5, -0.5),
            (0.5, 0.5, -0.5),
            (-0.5, 0.5, -0.5),
            (-0.5, -0.5, 0.5),
            (0.5, -0.5, 0.5),
            (0.5, 0.5, 0.5),
            (-0.5, 0.5, 0.5),
        ]
        pos, quat, scale = ab.y_up_to_blender(
            (0.0, 0.04, 0.0), (0.0, 0.0, 0.0, 1.0), (6.0, 0.08, 6.0)
        )
        self.assertEqual(scale, (6.0, 0.08, 6.0))
        xs, ys, zs = [], [], []
        for lx, ly, lz in local:
            # Blender object: scale local, then rotate, then translate.
            sx, sy, sz = scale[0] * lx, scale[1] * ly, scale[2] * lz
            wx, wy, wz = ab.qrot_xyzw(quat, (sx, sy, sz))
            xs.append(pos[0] + wx)
            ys.append(pos[1] + wy)
            zs.append(pos[2] + wz)
        ext_x = max(xs) - min(xs)
        ext_y = max(ys) - min(ys)
        ext_z = max(zs) - min(zs)
        # Floor: wide in Blender X/Y, thin along Z (up).
        self.assertAlmostEqual(ext_x, 6.0, places=4)
        self.assertAlmostEqual(ext_y, 6.0, places=4)
        self.assertAlmostEqual(ext_z, 0.08, places=4)
        self.assertLess(ext_z, ext_x * 0.1)
        self.assertLess(ext_z, ext_y * 0.1)

    def test_character_tall_y_stays_upright(self):
        """Tall Akasha Y limb must stay tall along Blender Z after convert."""
        pos, quat, scale = ab.y_up_to_blender(
            (0.0, 1.0, 0.0), (0.0, 0.0, 0.0, 1.0), (0.2, 0.8, 0.2)
        )
        local = [(0.0, -0.5, 0.0), (0.0, 0.5, 0.0)]  # local Y spine
        world = []
        for lx, ly, lz in local:
            sx, sy, sz = scale[0] * lx, scale[1] * ly, scale[2] * lz
            wx, wy, wz = ab.qrot_xyzw(quat, (sx, sy, sz))
            world.append((pos[0] + wx, pos[1] + wy, pos[2] + wz))
        dz = abs(world[1][2] - world[0][2])
        dy = abs(world[1][1] - world[0][1])
        dx = abs(world[1][0] - world[0][0])
        self.assertAlmostEqual(dz, 0.8, places=4)
        self.assertLess(dx, 0.05)
        self.assertLess(dy, 0.05)

    def test_world_compose_parented_camera(self):
        nodes = {
            "root": {
                "translation": [0, 0, 0],
                "rotation_xyzw": [0, 0, 0, 1],
                "scale": [1, 1, 1],
                "kind": "empty",
            },
            "camera": {
                "parent": "root",
                "translation": [0, 2.2, 6.5],
                "rotation_xyzw": [0, 0, 0, 1],
                "scale": [1, 1, 1],
                "kind": "camera",
            },
            "box": {
                "parent": "root",
                "translation": [0, 1.0, 0],
                "rotation_xyzw": [0, 0, 0, 1],
                "scale": [0.5, 1.0, 0.5],
                "kind": "mesh_box",
                "visible": True,
            },
        }
        world = ab.compute_akasha_world_trs(nodes)
        self.assertAlmostEqual(world["camera"][0][1], 2.2, places=5)
        centroid = ab.mesh_centroid_akasha(nodes, world)
        self.assertAlmostEqual(centroid[1], 1.0, places=5)
        eye_b, quat_b, _ = ab.y_up_to_blender(*world["camera"])
        target_b = ab.y_up_vec_to_blender(centroid)
        # Converted quat should already aim near +Y; look-at should too.
        look = ab.qrot_xyzw(quat_b, (0.0, 0.0, -1.0))
        self.assertGreater(look[1], 0.9)
        la = ab.look_at_quat_blender(eye_b, target_b)
        look_la = ab.qrot_xyzw(la, (0.0, 0.0, -1.0))
        # Direction from eye to target
        dx = target_b[0] - eye_b[0]
        dy = target_b[1] - eye_b[1]
        dz = target_b[2] - eye_b[2]
        ln = math.sqrt(dx * dx + dy * dy + dz * dz)
        dx, dy, dz = dx / ln, dy / ln, dz / ln
        self.assertAlmostEqual(look_la[0], dx, places=4)
        self.assertAlmostEqual(look_la[1], dy, places=4)
        self.assertAlmostEqual(look_la[2], dz, places=4)


if __name__ == "__main__":
    unittest.main()
