PASS_INSTRUCTIONS = {

    "gesture": """
Draw only the gesture structure.

Establish:
- line of action
- head direction
- shoulder axis
- hip axis
- limb directions
- balance

Use loose, light construction strokes.

Do not draw anatomy, clothing or details.
""",

    "skeleton": """
Continue the existing gesture drawing.

Add the structural drawing skeleton:
- head construction
- spine
- shoulders
- pelvis
- arms
- elbows
- wrists
- legs
- knees
- ankles

Keep the original gesture visible underneath.

Do not redraw the pose.
""",

    "mannequin": """
Continue directly from the existing skeleton.

Build simple three-dimensional volumes around it.

Add:
- head sphere and jaw
- rib cage
- pelvis
- shoulder masses
- arm cylinders
- leg cylinders
- hand wedges
- foot wedges
- cross-contours

The existing skeleton must remain visible as light construction.

Every volume must follow the existing joints.
""",

    "construction": """
Continue directly from the mannequin.

Connect the primitive volumes into a coherent human body construction.

Add:
- torso transitions
- shoulder connections
- hip connections
- limb transitions
- overlaps
- simplified body contour

Keep underlying construction lightly visible.

Do not change pose or proportions.
""",

    "anatomy": """
Continue directly from the body construction.

Add simplified major anatomical masses.

Prioritize:
- neck
- shoulders
- chest
- abdomen
- back
- upper arms
- forearms
- thighs
- calves

Do not exaggerate individual muscles.

Anatomy must wrap around the existing construction.
""",

    "clean_line": """
Extract a clean drawing from the existing construction.

Preserve exactly:
- pose
- proportions
- anatomy
- perspective
- silhouette

Add:
- clean contour
- facial structure
- hands
- feet
- hair
- clothing if specified

Fade construction lines but do not reinterpret the character.
""",

    "rendering": """
Render the existing clean drawing.

Do not modify the drawing structure.

Add:
- light
- form shadows
- cast shadows
- material definition
- texture
- final details

The established silhouette and anatomy are locked.
"""
}


BASE_INSTRUCTION = """
You are continuing an existing progressive character drawing.

CRITICAL RULE:

DO NOT redraw the character from scratch.

The supplied image represents the authoritative current drawing state.

Treat existing construction marks as structural constraints.

PRESERVE EXACTLY:

- canvas dimensions
- framing
- camera
- perspective
- character position
- line of action
- head position
- shoulder positions
- pelvis position
- elbow positions
- wrist positions
- knee positions
- ankle positions
- limb directions
- body proportions

The next image is the NEXT PASS of the SAME drawing.

Add information instead of replacing existing structure.

Never silently correct or reinterpret the pose.
"""


def build_prompt(
    drawing_pass: str,
    character_description: str = "",
):

    if drawing_pass not in PASS_INSTRUCTIONS:
        raise ValueError(
            f"Unknown drawing pass: {drawing_pass}"
        )

    prompt = BASE_INSTRUCTION

    if character_description:
        prompt += "\n\nCHARACTER:\n"
        prompt += character_description

    prompt += "\n\nCURRENT PASS:\n"
    prompt += PASS_INSTRUCTIONS[
        drawing_pass
    ]

    return prompt.strip()


if __name__ == "__main__":

    import argparse

    parser = argparse.ArgumentParser()

    parser.add_argument(
        "pass_name",
        choices=PASS_INSTRUCTIONS.keys(),
    )

    parser.add_argument(
        "--character",
        default="",
    )

    args = parser.parse_args()

    print(
        build_prompt(
            args.pass_name,
            args.character,
        )
    )
