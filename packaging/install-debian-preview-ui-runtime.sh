#!/usr/bin/env bash
# Install Linux runtime libraries for Preview egui (aos-ui-egui). Idempotent.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LIST="${ROOT}/packaging/debian-preview-ui-runtime.txt"
if [ ! -f "${LIST}" ]; then
  echo "missing ${LIST}" >&2
  exit 1
fi
mapfile -t PKGS < <(grep -v '^[[:space:]]*#' "${LIST}" | grep -v '^[[:space:]]*$' || true)
if [ "${#PKGS[@]}" -eq 0 ]; then
  echo "no packages listed in ${LIST}" >&2
  exit 1
fi
export DEBIAN_FRONTEND=noninteractive
sudo apt-get update -qq
sudo apt-get install -y -qq "${PKGS[@]}"
