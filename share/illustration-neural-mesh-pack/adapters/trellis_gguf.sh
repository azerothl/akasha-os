#!/usr/bin/env bash
# trellis_gguf.sh — Illustration Neural Mesh Pack adapter
#
# Argv matches trellis.cpp CLI:
#   trellis_gguf.sh <input.png> <output.glb> --models <GGUF_DIR> [--res 512|1024|1536] …
#
# Behaviour:
# 1. If AOS_NEURAL_MESH_ADAPTER_MOCK=1 → copy pack fixtures/unit_cube.glb (CI / dry-run).
# 2. Else if `trellis-cli` is on PATH or beside this script → exec real runner.
# 3. Else fail closed (no LocalAI HTTP from this adapter — caps prefer argv + net deny).
#
# Weights are never downloaded here. Install GGUFs offline first.

set -euo pipefail

usage() {
  echo "usage: $0 <input.png> <output.glb> --models <dir> [--res N] …" >&2
  exit 2
}

if [[ $# -lt 4 ]]; then
  usage
fi

INPUT="$1"
OUTPUT="$2"
shift 2

MODELS=""
RES="512"
EXTRA_ARGS=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --models)
      [[ $# -ge 2 ]] || usage
      MODELS="$2"
      shift 2
      ;;
    --res)
      [[ $# -ge 2 ]] || usage
      RES="$2"
      shift 2
      ;;
    --weights)
      # Legacy alias from earlier spike docs — map to --models.
      [[ $# -ge 2 ]] || usage
      MODELS="$2"
      shift 2
      ;;
    *)
      # Forward unknown flags to trellis-cli when present.
      EXTRA_ARGS+=("$1")
      shift
      ;;
  esac
done

if [[ -z "$MODELS" || ! -d "$MODELS" ]]; then
  echo "trellis_gguf: --models directory required and must exist" >&2
  exit 1
fi
if [[ ! -f "$INPUT" ]]; then
  echo "trellis_gguf: input image not found: $INPUT" >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PACK_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
FIXTURE="${AOS_NEURAL_MESH_FIXTURE:-${PACK_ROOT}/fixtures/unit_cube.glb}"

# CI / dry-run: produce a validated GLB without GGUF weights.
if [[ "${AOS_NEURAL_MESH_ADAPTER_MOCK:-}" == "1" ]]; then
  if [[ ! -f "$FIXTURE" ]]; then
    echo "trellis_gguf: mock fixture missing: $FIXTURE" >&2
    exit 1
  fi
  mkdir -p "$(dirname "$OUTPUT")"
  cp -f "$FIXTURE" "$OUTPUT"
  echo "trellis_gguf: mock → $OUTPUT (res=$RES models=$MODELS)" >&2
  exit 0
fi

find_trellis_cli() {
  if [[ -n "${AOS_NEURAL_MESH_TRELLIS_CLI:-}" && -x "${AOS_NEURAL_MESH_TRELLIS_CLI}" ]]; then
    echo "${AOS_NEURAL_MESH_TRELLIS_CLI}"
    return 0
  fi
  for cand in \
    "${PACK_ROOT}/bin/trellis-cli" \
    "${PACK_ROOT}/bin/trellis-cli.exe" \
    "$(command -v trellis-cli 2>/dev/null || true)"
  do
    if [[ -n "$cand" && -x "$cand" ]]; then
      echo "$cand"
      return 0
    fi
  done
  return 1
}

if CLI="$(find_trellis_cli)"; then
  mkdir -p "$(dirname "$OUTPUT")"
  exec "$CLI" "$INPUT" "$OUTPUT" --models "$MODELS" --res "$RES" ${EXTRA_ARGS[@]+"${EXTRA_ARGS[@]}"}
fi

echo "trellis_gguf: no trellis-cli found; install runner or set AOS_NEURAL_MESH_ADAPTER_MOCK=1" >&2
exit 1
