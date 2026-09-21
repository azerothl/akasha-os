import json
from dataclasses import dataclass
from typing import Dict, Tuple

from proportions import build_proportion_model


Point = Tuple[float, float]


@dataclass
class Skeleton:
    joints: Dict[str, Point]
    bones: list


def build_skeleton(
    height_heads: float = 7.5,
    shoulder_width_heads: float = 2.25,
) -> Skeleton:

    model = build_proportion_model(
        height_heads,
        shoulder_width_heads,
    )

    lm = model["landmarks"]
    widths = model["widths"]

    center_x = 0.5

    shoulder_half = widths["shoulders"] / 2
    pelvis_half = widths["pelvis"] / 2

    joints = {
        "head_top": (
            center_x,
            lm["top_head"],
        ),
        "head_center": (
            center_x,
            lm["chin"] * 0.5,
        ),
        "chin": (
            center_x,
            lm["chin"],
        ),
        "neck": (
            center_x,
            lm["shoulders"] - 0.025,
        ),
        "chest": (
            center_x,
            lm["chest"],
        ),
        "pelvis": (
            center_x,
            lm["pelvis"],
        ),
        "left_shoulder": (
            center_x - shoulder_half,
            lm["shoulders"],
        ),
        "right_shoulder": (
            center_x + shoulder_half,
            lm["shoulders"],
        ),
        "left_elbow": (
            center_x - shoulder_half * 1.15,
            lm["elbows"],
        ),
        "right_elbow": (
            center_x + shoulder_half * 1.15,
            lm["elbows"],
        ),
        "left_wrist": (
            center_x - shoulder_half * 1.10,
            lm["wrists"],
        ),
        "right_wrist": (
            center_x + shoulder_half * 1.10,
            lm["wrists"],
        ),
        "left_hip": (
            center_x - pelvis_half,
            lm["pelvis"],
        ),
        "right_hip": (
            center_x + pelvis_half,
            lm["pelvis"],
        ),
        "left_knee": (
            center_x - pelvis_half * 0.75,
            lm["knees"],
        ),
        "right_knee": (
            center_x + pelvis_half * 0.75,
            lm["knees"],
        ),
        "left_ankle": (
            center_x - pelvis_half * 0.55,
            lm["ankles"],
        ),
        "right_ankle": (
            center_x + pelvis_half * 0.55,
            lm["ankles"],
        ),
        "left_foot": (
            center_x - pelvis_half * 0.60,
            lm["ground"],
        ),
        "right_foot": (
            center_x + pelvis_half * 0.60,
            lm["ground"],
        ),
    }

    bones = [
        ("head_center", "chin"),
        ("chin", "neck"),
        ("neck", "chest"),
        ("chest", "pelvis"),
        ("neck", "left_shoulder"),
        ("left_shoulder", "left_elbow"),
        ("left_elbow", "left_wrist"),
        ("neck", "right_shoulder"),
        ("right_shoulder", "right_elbow"),
        ("right_elbow", "right_wrist"),
        ("pelvis", "left_hip"),
        ("left_hip", "left_knee"),
        ("left_knee", "left_ankle"),
        ("left_ankle", "left_foot"),
        ("pelvis", "right_hip"),
        ("right_hip", "right_knee"),
        ("right_knee", "right_ankle"),
        ("right_ankle", "right_foot"),
    ]

    return Skeleton(
        joints=joints,
        bones=bones,
    )


def serialize_skeleton(
    skeleton: Skeleton,
):
    return {
        "joints": {
            name: {
                "x": position[0],
                "y": position[1],
            }
            for name, position
            in skeleton.joints.items()
        },
        "bones": [
            {
                "from": start,
                "to": end,
            }
            for start, end
            in skeleton.bones
        ],
    }


if __name__ == "__main__":

    skeleton = build_skeleton()

    print(
        json.dumps(
            serialize_skeleton(skeleton),
            indent=2,
        )
    )
