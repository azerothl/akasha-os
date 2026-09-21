from math import sqrt

from skeleton import build_skeleton


def distance(a, b):

    return sqrt(
        (a[0] - b[0]) ** 2
        +
        (a[1] - b[1]) ** 2
    )


def validate_skeleton(skeleton):

    joints = skeleton.joints

    errors = []
    warnings = []

    # ---------------------------------------
    # Shoulder symmetry
    # ---------------------------------------

    left_arm = distance(
        joints["left_shoulder"],
        joints["left_elbow"],
    )

    right_arm = distance(
        joints["right_shoulder"],
        joints["right_elbow"],
    )

    difference = abs(
        left_arm - right_arm
    )

    if difference > 0.03:
        warnings.append(
            "Upper arm projected lengths differ significantly."
        )

    # ---------------------------------------
    # Leg sanity
    # ---------------------------------------

    left_leg = (
        distance(
            joints["left_hip"],
            joints["left_knee"],
        )
        +
        distance(
            joints["left_knee"],
            joints["left_ankle"],
        )
    )

    right_leg = (
        distance(
            joints["right_hip"],
            joints["right_knee"],
        )
        +
        distance(
            joints["right_knee"],
            joints["right_ankle"],
        )
    )

    if abs(left_leg - right_leg) > 0.05:
        warnings.append(
            "Projected leg lengths differ significantly."
        )

    # ---------------------------------------
    # Vertical ordering
    # ---------------------------------------

    if (
        joints["head_center"][1]
        >= joints["pelvis"][1]
    ):
        errors.append(
            "Head must remain above pelvis."
        )

    if (
        joints["left_knee"][1]
        <= joints["left_hip"][1]
    ):
        errors.append(
            "Left knee must remain below left hip."
        )

    if (
        joints["right_knee"][1]
        <= joints["right_hip"][1]
    ):
        errors.append(
            "Right knee must remain below right hip."
        )

    return {
        "valid": len(errors) == 0,
        "errors": errors,
        "warnings": warnings,
    }


if __name__ == "__main__":

    skeleton = build_skeleton()

    result = validate_skeleton(
        skeleton
    )

    print(result)
