#!/usr/bin/env bash
# Shared by the CUDA Linux release container and its CI regression job.
set -euo pipefail

VERIFIER_VENV="${RUNNER_TEMP:?RUNNER_TEMP must be set}/module-verifier-venv"
: "${GITHUB_PATH:?GITHUB_PATH must be set}"
python3 -m venv "$VERIFIER_VENV"
"$VERIFIER_VENV/bin/python" -m pip install "cryptography>=46,<50"
printf '%s\n' "$VERIFIER_VENV/bin" >> "$GITHUB_PATH"
