from dataclasses import dataclass
from typing import Dict


@dataclass
class BodyProportions:
    height_heads: float = 7.5
    shoulder_width_heads: float = 2.25


def calculate_landmarks(
    proportions: BodyProportions,
) -> Dict[str, float]:
    """
    Returns vertical landmarks expressed in normalized
    body coordinates.

    y = 0.0 -> top of head
    y = 1.0 -> ground
    """

    heads = proportions.height_heads

    def h(value: float) -> float:
        return value / heads

    landmarks = {
        "top_head": 0.0,
        "chin": h(1.0),
        "shoulders": h(1.45),
        "chest": h(2.0),
        "elbows": h(3.0),
        "navel": h(3.0),
        "pelvis": h(3.75),
        "wrists": h(3.8),
        "crotch": h(4.0),
        "knees": h(5.7),
        "ankles": h(heads - 0.25),
        "ground": 1.0,
    }

    return landmarks


def calculate_widths(
    proportions: BodyProportions,
) -> Dict[str, float]:
    """
    Widths are expressed relative to total body height.
    """

    head_width = 1.0 / proportions.height_heads

    shoulder_width = (
        proportions.shoulder_width_heads
        * head_width
    )

    pelvis_width = shoulder_width * 0.72

    return {
        "head": head_width * 0.72,
        "shoulders": shoulder_width,
        "chest": shoulder_width * 0.82,
        "waist": shoulder_width * 0.55,
        "pelvis": pelvis_width,
    }


def build_proportion_model(
    height_heads: float = 7.5,
    shoulder_width_heads: float = 2.25,
):
    proportions = BodyProportions(
        height_heads=height_heads,
        shoulder_width_heads=shoulder_width_heads,
    )

    return {
        "height_heads": height_heads,
        "landmarks": calculate_landmarks(proportions),
        "widths": calculate_widths(proportions),
    }


if __name__ == "__main__":
    import json

    model = build_proportion_model()

    print(
        json.dumps(
            model,
            indent=2,
        )
    )
