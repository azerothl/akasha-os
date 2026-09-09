#!/usr/bin/env bash
# build-create.sh — package the Create rich UI module (.aospkg)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../" && pwd)"
MOD="${ROOT}/modules/create"
STAGING="${ROOT}/modules/create.aospkg"
SHARE="${ROOT}/share/modules/create.aospkg"

echo "== build wasm32 (create) =="
TARGET="${CARGO_TARGET_DIR:-${ROOT}/target}"
CARGO_TARGET_DIR="${TARGET}" \
  cargo build --manifest-path "${MOD}/Cargo.toml" \
  --target wasm32-unknown-unknown --release

WASM_SRC=""
for cand in \
  "${TARGET}/wasm32-unknown-unknown/release/module_create.wasm" \
  "${ROOT}/target/wasm32-unknown-unknown/release/module_create.wasm"
do
  if [[ -f "$cand" ]]; then
    WASM_SRC="$cand"
    break
  fi
done
if [[ -z "$WASM_SRC" ]]; then
  echo "ERROR: module_create.wasm not found" >&2
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
name: create
version: 1.0.0
hash: ${HASH}
permissions:
  required_caps:
    - fs.read:/documents/create/**
    - fs.write:/documents/create/**
    - fs.read:/downloads/**
    - fs.write:/downloads/**
    - media.generate
    - tool.invoke:create
services:
  jobs: 1
  media_image: 1
tools:
  - name: create.history.list
    description: List generation history entries
    input_schema:
      type: object
  - name: create.history.get
    description: Fetch one history entry and restore params
    input_schema:
      type: object
      properties:
        id:
          type: string
      required: [id]
  - name: create.history.record
    description: Append a generation to history
    input_schema:
      type: object
  - name: create.document.load
    description: Load package document state
    input_schema:
      type: object
  - name: create.document.save
    description: Persist package document state
    input_schema:
      type: object
  - name: create.result.get
    description: Return last generated image path for preview
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
