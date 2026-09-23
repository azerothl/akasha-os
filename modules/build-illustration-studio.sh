#!/usr/bin/env bash
# build-illustration-studio.sh — package Illustration Studio (.aospkg)
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
# Product / design docs ship with the package (not hashed; catalogue hash is wasm-only).
if [[ -d "${MOD}/docs" ]]; then
  rm -rf "${STAGING}/docs"
  mkdir -p "${STAGING}/docs"
  cp -a "${MOD}/docs/." "${STAGING}/docs/"
fi
HASH="$(sha256_file "${STAGING}/module.wasm")"

sed "s/^hash: BUILD_HASH$/hash: ${HASH}/" "${MOD}/manifest.yaml" > "${STAGING}/manifest.yaml"

rm -rf "$SHARE"
cp -a "${STAGING}" "$SHARE"
echo "== package ready: ${STAGING} / ${SHARE} (hash ${HASH}) =="

# Keep local catalogue hash/caps in sync when present.
CATALOGUE="${ROOT}/share/modules/catalogue.yaml"
if [[ -f "$CATALOGUE" ]]; then
  echo "== update catalogue.yaml illustration-studio hash =="
  perl -i -0pe "s/(  - name: illustration-studio\r?\n(?:(?!  - name: ).)*?    hash: )sha256:[a-f0-9]+/\${1}sha256:${HASH}/s" "$CATALOGUE"
  VERSION="$(sed -n 's/^version: //p' "${MOD}/manifest.yaml" | head -n 1)"
  perl -i -0pe "s/(  - name: illustration-studio\r?\n    version: )\"[^\"]+\"/\${1}\"${VERSION}\"/" "$CATALOGUE"
  echo "== refresh catalogue signature (UPDATE_CATALOGUE=1) =="
  (cd "${ROOT}" && UPDATE_CATALOGUE=1 cargo test -p aos-platform --no-default-features \
    committed_catalogue_signature_matches -- --nocapture) \
    || echo "WARN: catalogue signature refresh failed — run UPDATE_CATALOGUE=1 cargo test -p aos-platform --no-default-features committed_catalogue_signature_matches"
fi
