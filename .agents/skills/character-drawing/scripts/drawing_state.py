import json
import hashlib
from pathlib import Path


PASS_ORDER = [
    "gesture",
    "skeleton",
    "mannequin",
    "construction",
    "anatomy",
    "clean_line",
    "rendering",
]


def create_state(
    character_id,
    geometry_path=None,
):

    return {
        "version": "1.0",
        "character_id": character_id,
        "current_pass": "gesture",
        "completed_passes": [],
        "locked": {
            "canvas": True,
            "camera": True,
            "proportions": True,
            "pose": True,
            "joints": True,
        },
        "geometry": {
            "source": geometry_path,
            "hash": None,
        },
        "drawing": {
            "previous_image": None,
            "current_image": None,
            "existing_strokes": [],
            "new_strokes": [
                "gesture"
            ],
        },
        "visibility": {
            "gesture": 1.0,
            "skeleton": 0.0,
            "mannequin": 0.0,
            "construction": 0.0,
            "anatomy": 0.0,
            "clean_line": 0.0,
            "rendering": 0.0,
        },
        "next_pass": "skeleton",
    }


def calculate_file_hash(path):

    path = Path(path)

    if not path.exists():
        return None

    sha = hashlib.sha256()

    with path.open("rb") as file:
        while True:

            chunk = file.read(
                8192
            )

            if not chunk:
                break

            sha.update(chunk)

    return sha.hexdigest()


def advance_state(
    state,
    generated_image,
):

    current = state[
        "current_pass"
    ]

    index = PASS_ORDER.index(
        current
    )

    # Current pass completed

    if current not in state[
        "completed_passes"
    ]:

        state[
            "completed_passes"
        ].append(
            current
        )

    previous_image = state[
        "drawing"
    ][
        "current_image"
    ]

    state[
        "drawing"
    ][
        "previous_image"
    ] = previous_image

    state[
        "drawing"
    ][
        "current_image"
    ] = generated_image

    # Existing strokes

    if current not in state[
        "drawing"
    ][
        "existing_strokes"
    ]:

        state[
            "drawing"
        ][
            "existing_strokes"
        ].append(
            current
        )

    # Final pass

    if index >= len(
        PASS_ORDER
    ) - 1:

        state[
            "next_pass"
        ] = None

        state[
            "drawing"
        ][
            "new_strokes"
        ] = []

        return state

    next_pass = PASS_ORDER[
        index + 1
    ]

    state[
        "current_pass"
    ] = next_pass

    state[
        "next_pass"
    ] = (
        PASS_ORDER[index + 2]
        if index + 2 < len(PASS_ORDER)
        else None
    )

    state[
        "drawing"
    ][
        "new_strokes"
    ] = [
        next_pass
    ]

    update_visibility(
        state
    )

    return state


def update_visibility(state):

    current = state[
        "current_pass"
    ]

    visibility = {
        "gesture": 0.0,
        "skeleton": 0.0,
        "mannequin": 0.0,
        "construction": 0.0,
        "anatomy": 0.0,
        "clean_line": 0.0,
        "rendering": 0.0,
    }

    if current == "gesture":

        visibility[
            "gesture"
        ] = 1.0

    elif current == "skeleton":

        visibility[
            "gesture"
        ] = 0.35

        visibility[
            "skeleton"
        ] = 1.0

    elif current == "mannequin":

        visibility[
            "gesture"
        ] = 0.15

        visibility[
            "skeleton"
        ] = 0.35

        visibility[
            "mannequin"
        ] = 1.0

    elif current == "construction":

        visibility[
            "skeleton"
        ] = 0.15

        visibility[
            "mannequin"
        ] = 0.35

        visibility[
            "construction"
        ] = 1.0

    elif current == "anatomy":

        visibility[
            "mannequin"
        ] = 0.20

        visibility[
            "construction"
        ] = 0.40

        visibility[
            "anatomy"
        ] = 1.0

    elif current == "clean_line":

        visibility[
            "construction"
        ] = 0.15

        visibility[
            "anatomy"
        ] = 0.25

        visibility[
            "clean_line"
        ] = 1.0

    elif current == "rendering":

        visibility[
            "clean_line"
        ] = 1.0

        visibility[
            "rendering"
        ] = 1.0

    state[
        "visibility"
    ] = visibility


def save_state(
    state,
    path,
):

    with open(
        path,
        "w",
        encoding="utf-8",
    ) as file:

        json.dump(
            state,
            file,
            indent=2,
        )


def load_state(path):

    with open(
        path,
        "r",
        encoding="utf-8",
    ) as file:

        return json.load(
            file
        )


if __name__ == "__main__":

    state = create_state(
        "character_001"
    )

    save_state(
        state,
        "drawing-state.json",
    )

    print(
        json.dumps(
            state,
            indent=2,
        )
    )
