#!/usr/bin/env bash
# build-canvas.sh — build and package the canvas WASM module (.aospkg)
#
# canvas is its own Cargo workspace (modules/canvas/Cargo.toml [workspace]).
# Never prefer $CARGO_TARGET_DIR from the host packaging tree — that path often
# holds a stale module_canvas.wasm from an older checkout, which silently drops
# canvas.path from the packaged guest (filter then hides the tool).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../" && pwd)"
CANVAS_DIR="${ROOT}/modules/canvas"
SHARE_PKG="${ROOT}/share/modules/canvas.aospkg"
STAGING="${ROOT}/modules/canvas.aospkg"
# Own target dir so packaging's CARGO_TARGET_DIR cannot shadow a fresh build.
CANVAS_TARGET="${CANVAS_DIR}/target"

echo "== build wasm32 (canvas) =="
CARGO_TARGET_DIR="${CANVAS_TARGET}" \
  cargo build --manifest-path "${CANVAS_DIR}/Cargo.toml" \
  --target wasm32-unknown-unknown --release

WASM_SRC="${CANVAS_TARGET}/wasm32-unknown-unknown/release/module_canvas.wasm"
if [[ ! -f "$WASM_SRC" ]]; then
  echo "WASM artifact not found: ${WASM_SRC}" >&2
  exit 1
fi

mkdir -p "${STAGING}/ui"
cp "$WASM_SRC" "${STAGING}/module.wasm"

sha256_file() {
  local f="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$f" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$f" | awk '{print $1}'
  else
    echo "ERROR: sha256sum or shasum required" >&2
    exit 1
  fi
}
HASH="$(sha256_file "${STAGING}/module.wasm")"
MANIFEST_TEMPLATE="${SHARE_PKG}/manifest.yaml"
if [[ ! -f "$MANIFEST_TEMPLATE" ]]; then
  echo "manifest template missing: $MANIFEST_TEMPLATE" >&2
  exit 1
fi
sed "s/^hash: .*/hash: ${HASH}/" "$MANIFEST_TEMPLATE" > "${STAGING}/manifest.yaml"

if [[ -f "${SHARE_PKG}/ui/index.html" ]]; then
  cp "${SHARE_PKG}/ui/index.html" "${STAGING}/ui/index.html"
else
  cat > "${STAGING}/ui/index.html" <<'EOF'
{
  "type": "declarative_ui",
  "title": "Canvas",
  "description": "Session drawing canvas — surface is the Chat panel, not a sidebar tab."
}
EOF
fi

rm -rf "$SHARE_PKG"
cp -a "${STAGING}" "$SHARE_PKG"
echo "== package ready: ${SHARE_PKG} (hash ${HASH}) =="

CATALOGUE="${ROOT}/share/modules/catalogue.yaml"
if [[ -f "$CATALOGUE" ]] && grep -q "name: canvas" "$CATALOGUE"; then
  echo "== update catalogue.yaml canvas hash =="
  perl -i -0pe "s/(  - name: canvas\n(?:    .*\n)*?    hash: )sha256:[a-f0-9]+/\${1}sha256:${HASH}/" "$CATALOGUE"
  if command -v cargo >/dev/null 2>&1; then
    (cd "${ROOT}" && UPDATE_CATALOGUE=1 cargo test -p aos-platform --no-default-features committed_catalogue -- --nocapture) \
      || echo "WARN: catalogue signature refresh failed — run UPDATE_CATALOGUE=1 cargo test -p aos-platform --no-default-features committed_catalogue"
  fi
fi
