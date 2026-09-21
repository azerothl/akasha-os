from pathlib import Path
from math import atan2, cos, sin, pi

from PIL import Image, ImageDraw

from skeleton import build_skeleton


WIDTH = 768
HEIGHT = 1024

MARGIN_X = 130
MARGIN_Y = 70

BACKGROUND = (250, 249, 247)

GESTURE_COLOR = (205, 80, 80)
SKELETON_COLOR = (70, 105, 180)
VOLUME_COLOR = (90, 130, 160)
CONSTRUCTION_COLOR = (55, 55, 55)

LIGHT_GUIDE = (170, 170, 170)


# --------------------------------------------------
# Coordinate system
# --------------------------------------------------

def project(point):
    """
    Convert normalized body coordinates into canvas pixels.

    Normalized:
        x = 0..1
        y = 0..1
    """

    x, y = point

    px = (
        MARGIN_X
        + x * (WIDTH - MARGIN_X * 2)
    )

    py = (
        MARGIN_Y
        + y * (HEIGHT - MARGIN_Y * 2)
    )

    return (
        int(px),
        int(py),
    )


# --------------------------------------------------
# Basic drawing utilities
# --------------------------------------------------

def draw_joint(
    draw,
    point,
    radius=6,
    fill=SKELETON_COLOR,
):

    x, y = project(point)

    draw.ellipse(
        (
            x - radius,
            y - radius,
            x + radius,
            y + radius,
        ),
        fill=fill,
    )


def draw_bone(
    draw,
    start,
    end,
    fill=SKELETON_COLOR,
    width=4,
):

    draw.line(
        (
            project(start),
            project(end),
        ),
        fill=fill,
        width=width,
    )


def draw_axis(
    draw,
    start,
    end,
    fill=GESTURE_COLOR,
    width=3,
):

    draw.line(
        (
            project(start),
            project(end),
        ),
        fill=fill,
        width=width,
    )


def interpolate(a, b, t):

    return (
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
    )


# --------------------------------------------------
# Ellipse / body mass helpers
# --------------------------------------------------

def ellipse_from_center(
    draw,
    center,
    width,
    height,
    outline,
    line_width=3,
):

    cx, cy = project(center)

    scale_x = WIDTH - MARGIN_X * 2
    scale_y = HEIGHT - MARGIN_Y * 2

    rx = width * scale_x / 2
    ry = height * scale_y / 2

    box = (
        int(cx - rx),
        int(cy - ry),
        int(cx + rx),
        int(cy + ry),
    )

    draw.ellipse(
        box,
        outline=outline,
        width=line_width,
    )


def draw_cross_contour(
    draw,
    center,
    width,
    color=LIGHT_GUIDE,
):

    cx, cy = project(center)

    scale_x = WIDTH - MARGIN_X * 2

    radius = width * scale_x / 2

    draw.arc(
        (
            int(cx - radius),
            int(cy - radius * 0.35),
            int(cx + radius),
            int(cy + radius * 0.35),
        ),
        0,
        180,
        fill=color,
        width=2,
    )


# --------------------------------------------------
# Limb volume
# --------------------------------------------------

def draw_limb_volume(
    draw,
    start,
    end,
    radius_start,
    radius_end,
    outline=VOLUME_COLOR,
    line_width=3,
):

    """
    Draw a tapered cylinder-like 2D projection.
    """

    sx, sy = project(start)
    ex, ey = project(end)

    dx = ex - sx
    dy = ey - sy

    angle = atan2(dy, dx)

    perpendicular = angle + pi / 2

    scale = WIDTH - MARGIN_X * 2

    r1 = radius_start * scale
    r2 = radius_end * scale

    p1 = (
        sx + cos(perpendicular) * r1,
        sy + sin(perpendicular) * r1,
    )

    p2 = (
        sx - cos(perpendicular) * r1,
        sy - sin(perpendicular) * r1,
    )

    p3 = (
        ex - cos(perpendicular) * r2,
        ey - sin(perpendicular) * r2,
    )

    p4 = (
        ex + cos(perpendicular) * r2,
        ey + sin(perpendicular) * r2,
    )

    polygon = [
        p1,
        p2,
        p3,
        p4,
    ]

    draw.line(
        polygon + [polygon[0]],
        fill=outline,
        width=line_width,
        joint="curve",
    )

    # Joint ellipses

    draw.ellipse(
        (
            sx - r1,
            sy - r1,
            sx + r1,
            sy + r1,
        ),
        outline=outline,
        width=2,
    )

    draw.ellipse(
        (
            ex - r2,
            ey - r2,
            ex + r2,
            ey + r2,
        ),
        outline=outline,
        width=2,
    )


# --------------------------------------------------
# Gesture pass
# --------------------------------------------------

def render_gesture(
    skeleton,
    output_path,
):

    image = Image.new(
        "RGB",
        (WIDTH, HEIGHT),
        BACKGROUND,
    )

    draw = ImageDraw.Draw(image)

    j = skeleton.joints

    # -----------------------------
    # Head gesture
    # -----------------------------

    ellipse_from_center(
        draw,
        j["head_center"],
        0.11,
        0.12,
        GESTURE_COLOR,
        3,
    )

    # -----------------------------
    # Main line of action
    # -----------------------------

    points = [
        project(j["head_center"]),
        project(j["neck"]),
        project(j["chest"]),
        project(j["pelvis"]),
    ]

    draw.line(
        points,
        fill=GESTURE_COLOR,
        width=5,
        joint="curve",
    )

    # -----------------------------
    # Shoulder axis
    # -----------------------------

    draw_axis(
        draw,
        j["left_shoulder"],
        j["right_shoulder"],
    )

    # -----------------------------
    # Hip axis
    # -----------------------------

    draw_axis(
        draw,
        j["left_hip"],
        j["right_hip"],
    )

    # -----------------------------
    # Arms
    # -----------------------------

    for side in (
        "left",
        "right",
    ):

        draw_axis(
            draw,
            j[f"{side}_shoulder"],
            j[f"{side}_elbow"],
        )

        draw_axis(
            draw,
            j[f"{side}_elbow"],
            j[f"{side}_wrist"],
        )

    # -----------------------------
    # Legs
    # -----------------------------

    for side in (
        "left",
        "right",
    ):

        draw_axis(
            draw,
            j[f"{side}_hip"],
            j[f"{side}_knee"],
        )

        draw_axis(
            draw,
            j[f"{side}_knee"],
            j[f"{side}_ankle"],
        )

        draw_axis(
            draw,
            j[f"{side}_ankle"],
            j[f"{side}_foot"],
        )

    image.save(output_path)


# --------------------------------------------------
# Skeleton pass
# --------------------------------------------------

def render_skeleton(
    skeleton,
    output_path,
):

    image = Image.new(
        "RGB",
        (WIDTH, HEIGHT),
        BACKGROUND,
    )

    draw = ImageDraw.Draw(image)

    j = skeleton.joints

    # -----------------------------
    # Light gesture underneath
    # -----------------------------

    gesture_color = (
        225,
        180,
        180,
    )

    draw.line(
        [
            project(j["head_center"]),
            project(j["neck"]),
            project(j["chest"]),
            project(j["pelvis"]),
        ],
        fill=gesture_color,
        width=3,
    )

    # -----------------------------
    # Bones
    # -----------------------------

    for start_name, end_name in skeleton.bones:

        draw_bone(
            draw,
            j[start_name],
            j[end_name],
        )

    # -----------------------------
    # Joints
    # -----------------------------

    for point in j.values():

        draw_joint(
            draw,
            point,
            radius=5,
        )

    # -----------------------------
    # Head
    # -----------------------------

    ellipse_from_center(
        draw,
        j["head_center"],
        0.11,
        0.12,
        SKELETON_COLOR,
        3,
    )

    # Face direction guide

    head = project(
        j["head_center"]
    )

    chin = project(
        j["chin"]
    )

    draw.line(
        (head, chin),
        fill=LIGHT_GUIDE,
        width=2,
    )

    image.save(output_path)


# --------------------------------------------------
# Mannequin pass
# --------------------------------------------------

def render_mannequin(
    skeleton,
    output_path,
):

    image = Image.new(
        "RGB",
        (WIDTH, HEIGHT),
        BACKGROUND,
    )

    draw = ImageDraw.Draw(image)

    j = skeleton.joints

    # -----------------------------
    # Skeleton underneath
    # -----------------------------

    for start_name, end_name in skeleton.bones:

        draw_bone(
            draw,
            j[start_name],
            j[end_name],
            fill=(180, 195, 210),
            width=2,
        )

    # -----------------------------
    # Head
    # -----------------------------

    ellipse_from_center(
        draw,
        j["head_center"],
        0.12,
        0.13,
        VOLUME_COLOR,
        3,
    )

    draw_cross_contour(
        draw,
        j["head_center"],
        0.11,
    )

    # -----------------------------
    # Rib cage
    # -----------------------------

    rib_center = interpolate(
        j["chest"],
        j["pelvis"],
        0.28,
    )

    ellipse_from_center(
        draw,
        rib_center,
        0.29,
        0.27,
        VOLUME_COLOR,
        3,
    )

    draw_cross_contour(
        draw,
        rib_center,
        0.27,
    )

    # -----------------------------
    # Pelvis
    # -----------------------------

    ellipse_from_center(
        draw,
        j["pelvis"],
        0.23,
        0.15,
        VOLUME_COLOR,
        3,
    )

    draw_cross_contour(
        draw,
        j["pelvis"],
        0.21,
    )

    # -----------------------------
    # Arms
    # -----------------------------

    for side in (
        "left",
        "right",
    ):

        draw_limb_volume(
            draw,
            j[f"{side}_shoulder"],
            j[f"{side}_elbow"],
            0.040,
            0.030,
        )

        draw_limb_volume(
            draw,
            j[f"{side}_elbow"],
            j[f"{side}_wrist"],
            0.032,
            0.022,
        )

        # Hand

        ellipse_from_center(
            draw,
            j[f"{side}_wrist"],
            0.055,
            0.075,
            VOLUME_COLOR,
            2,
        )

    # -----------------------------
    # Legs
    # -----------------------------

    for side in (
        "left",
        "right",
    ):

        draw_limb_volume(
            draw,
            j[f"{side}_hip"],
            j[f"{side}_knee"],
            0.060,
            0.042,
        )

        draw_limb_volume(
            draw,
            j[f"{side}_knee"],
            j[f"{side}_ankle"],
            0.045,
            0.027,
        )

        # Foot

        ankle = j[f"{side}_ankle"]
        foot = j[f"{side}_foot"]

        draw_limb_volume(
            draw,
            ankle,
            foot,
            0.030,
            0.040,
            line_width=2,
        )

    image.save(output_path)


# --------------------------------------------------
# Construction pass
# --------------------------------------------------

def render_construction(
    skeleton,
    output_path,
):

    """
    Construction pass builds a simplified body contour
    while keeping the mannequin visible underneath.
    """

    image = Image.new(
        "RGB",
        (WIDTH, HEIGHT),
        BACKGROUND,
    )

    draw = ImageDraw.Draw(image)

    j = skeleton.joints

    # -----------------------------
    # Underlying skeleton
    # -----------------------------

    for start_name, end_name in skeleton.bones:

        draw_bone(
            draw,
            j[start_name],
            j[end_name],
            fill=(215, 220, 225),
            width=2,
        )

    # -----------------------------
    # Head
    # -----------------------------

    ellipse_from_center(
        draw,
        j["head_center"],
        0.12,
        0.13,
        LIGHT_GUIDE,
        2,
    )

    # -----------------------------
    # Rib cage
    # -----------------------------

    rib_center = interpolate(
        j["chest"],
        j["pelvis"],
        0.28,
    )

    ellipse_from_center(
        draw,
        rib_center,
        0.29,
        0.27,
        LIGHT_GUIDE,
        2,
    )

    # -----------------------------
    # Pelvis
    # -----------------------------

    ellipse_from_center(
        draw,
        j["pelvis"],
        0.23,
        0.15,
        LIGHT_GUIDE,
        2,
    )

    # -----------------------------
    # Body contour approximation
    # -----------------------------

    left_shoulder = project(
        j["left_shoulder"]
    )

    right_shoulder = project(
        j["right_shoulder"]
    )

    left_hip = project(
        j["left_hip"]
    )

    right_hip = project(
        j["right_hip"]
    )

    # Torso sides

    draw.line(
        (
            left_shoulder,
            left_hip,
        ),
        fill=CONSTRUCTION_COLOR,
        width=4,
    )

    draw.line(
        (
            right_shoulder,
            right_hip,
        ),
        fill=CONSTRUCTION_COLOR,
        width=4,
    )

    # Shoulder contour

    draw.line(
        (
            left_shoulder,
            right_shoulder,
        ),
        fill=CONSTRUCTION_COLOR,
        width=3,
    )

    # Pelvis contour

    draw.line(
        (
            left_hip,
            right_hip,
        ),
        fill=CONSTRUCTION_COLOR,
        width=3,
    )

    # -----------------------------
    # Limb contours
    # -----------------------------

    for side in (
        "left",
        "right",
    ):

        draw_limb_volume(
            draw,
            j[f"{side}_shoulder"],
            j[f"{side}_elbow"],
            0.044,
            0.032,
            outline=CONSTRUCTION_COLOR,
            line_width=3,
        )

        draw_limb_volume(
            draw,
            j[f"{side}_elbow"],
            j[f"{side}_wrist"],
            0.034,
            0.024,
            outline=CONSTRUCTION_COLOR,
            line_width=3,
        )

        draw_limb_volume(
            draw,
            j[f"{side}_hip"],
            j[f"{side}_knee"],
            0.064,
            0.044,
            outline=CONSTRUCTION_COLOR,
            line_width=3,
        )

        draw_limb_volume(
            draw,
            j[f"{side}_knee"],
            j[f"{side}_ankle"],
            0.048,
            0.028,
            outline=CONSTRUCTION_COLOR,
            line_width=3,
        )

    image.save(output_path)


# --------------------------------------------------
# Complete sequence
# --------------------------------------------------

def render_sequence(
    output_directory,
):

    output_directory = Path(
        output_directory
    )

    output_directory.mkdir(
        parents=True,
        exist_ok=True,
    )

    skeleton = build_skeleton()

    render_gesture(
        skeleton,
        output_directory / "01_gesture.png",
    )

    render_skeleton(
        skeleton,
        output_directory / "02_skeleton.png",
    )

    render_mannequin(
        skeleton,
        output_directory / "03_mannequin.png",
    )

    render_construction(
        skeleton,
        output_directory / "04_construction.png",
    )

    print(
        f"Drawing passes generated in: "
        f"{output_directory.resolve()}"
    )


if __name__ == "__main__":

    render_sequence(
        "output"
    )
