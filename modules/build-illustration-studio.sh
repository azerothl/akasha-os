#!/usr/bin/env bash
# build-illustration-studio.sh — package Illustration Studio foundation (.aospkg)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../" && pwd)"
MOD="${ROOT}/modules/illustration-studio"
STAGING="${ROOT}/modules/illustration-studio.aospkg"
SHARE="${ROOT}/share/modules/illustration-studio.aospkg"

echo "== build wasm32 (illustration-studio) =="
TARGET="${CARGO_TARGET_DIR:-${ROOT}/target}"
CARGO_TARGET_DIR="${TARGET}" \
  cargo build --manifest-path "${MOD}/Cargo.toml" \
  --target wasm32-unknown-unknown --release

WASM_SRC=""
for cand in \
  "${TARGET}/wasm32-unknown-unknown/release/module_illustration_studio.wasm" \
  "${ROOT}/target/wasm32-unknown-unknown/release/module_illustration_studio.wasm"
do
  if [[ -f "$cand" ]]; then
    WASM_SRC="$cand"
    break
  fi
done
if [[ -z "$WASM_SRC" ]]; then
  echo "ERROR: module_illustration_studio.wasm not found" >&2
  exit 1
fi

sha256_file() {
  local f="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$f" | awk '{print $1}'
  else
    shasum -a 256 "$f" | awk '{print $1}'
  fi
}

mkdir -p "${STAGING}/ui"
cp -f "$WASM_SRC" "${STAGING}/module.wasm"
cp -f "${MOD}/ui/index.json" "${STAGING}/ui/index.json"
HASH="$(sha256_file "${STAGING}/module.wasm")"

cat > "${STAGING}/manifest.yaml" <<EOF
name: illustration-studio
version: 0.1.0
hash: ${HASH}
permissions:
  required_caps:
    - fs.read:/documents/illustrations/**
    - fs.write:/documents/illustrations/**
    - render.stub
    - tool.invoke:illustration-studio
tools:
  - name: illustration.project.load
    description: Load SceneGraph project YAML
    input_schema:
      type: object
  - name: illustration.project.save
    description: Persist SceneGraph project YAML
    input_schema:
      type: object
      properties:
        yaml:
          type: string
      required: [yaml]
  - name: illustration.project.ensure
    description: Ensure demo project YAML exists
    input_schema:
      type: object
  - name: illustration.document.load
    description: Load package UI state
    input_schema:
      type: object
  - name: illustration.document.save
    description: Persist package UI state
    input_schema:
      type: object
ui:
  contract: 2
  document: ui/index.json
  entry: ui/index.json
  mode: declarative_ui
min_os_api: 1
EOF

rm -rf "$SHARE"
cp -a "${STAGING}" "$SHARE"
echo "== package ready: ${STAGING} / ${SHARE} (hash ${HASH}) =="
