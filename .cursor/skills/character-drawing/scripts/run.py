import argparse
import json
from pathlib import Path

import yaml

from skeleton import (
    build_skeleton,
    serialize_skeleton,
)

from validate_pose import (
    validate_skeleton,
)

from drawing_state import (
    create_state,
    save_state,
)

from render_guides import (
    render_gesture,
    render_skeleton,
    render_mannequin,
    render_construction,
)

from build_prompt import (
    build_prompt,
)


def load_character_spec(
    path,
):

    with open(
        path,
        "r",
        encoding="utf-8",
    ) as file:

        return yaml.safe_load(
            file
        )


def main():

    parser = argparse.ArgumentParser(
        description=(
            "Character Drawing Skill V1"
        )
    )

    parser.add_argument(
        "character_spec",
    )

    parser.add_argument(
        "--output",
        default="output",
    )

    args = parser.parse_args()

    # -----------------------------------
    # Load specification
    # -----------------------------------

    spec = load_character_spec(
        args.character_spec
    )

    output = Path(
        args.output
    )

    output.mkdir(
        parents=True,
        exist_ok=True,
    )

    # -----------------------------------
    # Proportions
    # -----------------------------------

    proportions = spec.get(
        "proportions",
        {}
    )

    height_heads = proportions.get(
        "height_heads",
        7.5,
    )

    shoulder_width = proportions.get(
        "shoulder_width_heads",
        2.25,
    )

    # -----------------------------------
    # Resolve skeleton
    # -----------------------------------

    skeleton = build_skeleton(
        height_heads,
        shoulder_width,
    )

    geometry = serialize_skeleton(
        skeleton
    )

    geometry_path = (
        output
        / "resolved-character.json"
    )

    with geometry_path.open(
        "w",
        encoding="utf-8",
    ) as file:

        json.dump(
            geometry,
            file,
            indent=2,
        )

    # -----------------------------------
    # Validate
    # -----------------------------------

    validation = validate_skeleton(
        skeleton
    )

    validation_path = (
        output
        / "validation.json"
    )

    with validation_path.open(
        "w",
        encoding="utf-8",
    ) as file:

        json.dump(
            validation,
            file,
            indent=2,
        )

    if not validation[
        "valid"
    ]:

        print(
            "Invalid skeleton."
        )

        for error in validation[
            "errors"
        ]:

            print(
                "ERROR:",
                error,
            )

        return

    # -----------------------------------
    # Render deterministic passes
    # -----------------------------------

    render_gesture(
        skeleton,
        output
        / "01_gesture.png",
    )

    render_skeleton(
        skeleton,
        output
        / "02_skeleton.png",
    )

    render_mannequin(
        skeleton,
        output
        / "03_mannequin.png",
    )

    render_construction(
        skeleton,
        output
        / "04_construction.png",
    )

    # -----------------------------------
    # Drawing state
    # -----------------------------------

    state = create_state(
        "character_001",
        str(
            geometry_path
        ),
    )

    state[
        "drawing"
    ][
        "current_image"
    ] = str(
        output
        / "01_gesture.png"
    )

    save_state(
        state,
        output
        / "drawing-state.json",
    )

    # -----------------------------------
    # Generate image-model prompts
    # -----------------------------------

    prompts_directory = (
        output
        / "prompts"
    )

    prompts_directory.mkdir(
        exist_ok=True,
    )

    passes = [
        "gesture",
        "skeleton",
        "mannequin",
        "construction",
        "anatomy",
        "clean_line",
        "rendering",
    ]

    character = spec.get(
        "character",
        {}
    )

    description = (
        f"{character.get('age_group', 'adult')} "
        f"{character.get('species', 'human')}, "
        f"{character.get('body', {}).get('build', 'average')} build"
    )

    for drawing_pass in passes:

        prompt = build_prompt(
            drawing_pass,
            description,
        )

        prompt_path = (
            prompts_directory
            / f"{drawing_pass}.txt"
        )

        prompt_path.write_text(
            prompt,
            encoding="utf-8",
        )

    print()
    print(
        "Character Drawing workspace created."
    )

    print()
    print(
        f"Output: {output.resolve()}"
    )

    print()
    print(
        "Generated:"
    )

    print(
        "  resolved-character.json"
    )

    print(
        "  validation.json"
    )

    print(
        "  drawing-state.json"
    )

    print(
        "  01_gesture.png"
    )

    print(
        "  02_skeleton.png"
    )

    print(
        "  03_mannequin.png"
    )

    print(
        "  04_construction.png"
    )

    print(
        "  prompts/*"
    )


if __name__ == "__main__":

    main()
