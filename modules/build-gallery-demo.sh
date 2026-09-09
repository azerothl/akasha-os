#!/usr/bin/env bash
# build-gallery-demo.sh — package the gallery-demo rich UI sample (.aospkg)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../" && pwd)"
MOD="${ROOT}/modules/gallery-demo"
STAGING="${ROOT}/modules/gallery-demo.aospkg"
SHARE="${ROOT}/share/modules/gallery-demo.aospkg"

echo "== build wasm32 (gallery-demo) =="
TARGET="${CARGO_TARGET_DIR:-${ROOT}/target}"
CARGO_TARGET_DIR="${TARGET}" \
  cargo build --manifest-path "${MOD}/Cargo.toml" \
  --target wasm32-unknown-unknown --release

WASM_SRC=""
for cand in \
  "${TARGET}/wasm32-unknown-unknown/release/module_gallery_demo.wasm" \
  "${ROOT}/target/wasm32-unknown-unknown/release/module_gallery_demo.wasm"
do
  if [[ -f "$cand" ]]; then
    WASM_SRC="$cand"
    break
  fi
done
if [[ -z "$WASM_SRC" ]]; then
  echo "ERROR: module_gallery_demo.wasm not found" >&2
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
name: gallery-demo
version: 1.0.0
hash: ${HASH}
permissions:
  required_caps:
    - fs.read:/documents/gallery-demo/**
    - fs.write:/documents/gallery-demo/**
    - tool.invoke:gallery-demo
services:
  jobs: 1
tools:
  - name: gallery-demo.preview.ensure
    description: Ensure sample preview PNG exists
    input_schema:
      type: object
  - name: gallery-demo.preview.get
    description: Return authorized preview image path
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
