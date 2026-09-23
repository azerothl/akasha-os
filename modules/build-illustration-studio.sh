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

cat > "${STAGING}/manifest.yaml" <<EOF
name: illustration-studio
version: 0.7.16
hash: ${HASH}
permissions:
  required_caps:
    - fs.read:/documents/illustrations/**
    - fs.write:/documents/illustrations/**
    - render.stub
    - render.cpu
    - render.blender
    - illustration.dependencies.install
    - illustration.asset.import
    - asset.read:/assets/illustration/**
    - scene.compose
    - scene.pose
    - scene.edit
    - scene.lock
    - mesh.neural
    - comic.layout
    - comic.render
    - storyboard.edit
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
  - name: scene.get
    description: Read SceneGraph snapshot (yaml + selection + locks)
    input_schema:
      type: object
      properties:
        scene_yaml:
          type: string
  - name: scene.select
    description: Select a SceneGraph node
    input_schema:
      type: object
      properties:
        id:
          type: string
        scene_yaml:
          type: string
      required: [id]
  - name: scene.trs
    description: Set node translation/rotation/scale (ADR 0011)
    input_schema:
      type: object
      properties:
        id:
          type: string
        translation:
          type: object
        rotation:
          type: object
        scale:
          type: object
        scene_yaml:
          type: string
      required: [id]
  - name: scene.camera
    description: Set active camera eye/look-at/orbit/FOV (ADR 0011)
    input_schema:
      type: object
      properties:
        id:
          type: string
        eye:
          type: object
        look_at:
          type: object
        fov_deg:
          type: number
        yaw:
          type: number
        pitch:
          type: number
        distance:
          type: number
        scene_yaml:
          type: string
  - name: scene.light
    description: Add or edit SceneGraph Light nodes (type/intensity/color)
    input_schema:
      type: object
      properties:
        id:
          type: string
        add:
          type: boolean
        parent_id:
          type: string
        light_type:
          type: string
        intensity:
          type: number
        color_srgb:
          type: array
        color_r:
          type: number
        color_g:
          type: number
        color_b:
          type: number
        scene_yaml:
          type: string
  - name: scene.apply
    description: Transactional agent edit batch (apply or rollback)
    input_schema:
      type: object
      properties:
        ops:
          type: array
        scene_yaml:
          type: string
      required: [ops]
  - name: scene.lock
    description: Semantic lock a node or subtree (agent must not mutate)
    input_schema:
      type: object
      properties:
        id:
          type: string
        scope:
          type: string
        kind:
          type: string
        scene_yaml:
          type: string
      required: [id]
  - name: scene.unlock
    description: Clear a semantic lock
    input_schema:
      type: object
      properties:
        id:
          type: string
        scene_yaml:
          type: string
      required: [id]
  - name: scene.locks
    description: List semantic locks
    input_schema:
      type: object
      properties:
        scene_yaml:
          type: string
  - name: scene.compose
    description: Compose SceneGraph from a short prompt
    input_schema:
      type: object
      properties:
        prompt:
          type: string
      required: [prompt]
  - name: scene.pose
    description: Apply pose/IK-lite to a humanoid
    input_schema:
      type: object
      properties:
        humanoid_root:
          type: string
        preset:
          type: string
        scene_yaml:
          type: string
  - name: scene.instantiate
    description: Instantiate an illustration asset into the SceneGraph
    input_schema:
      type: object
      properties:
        asset_id:
          type: string
        parent_id:
          type: string
        prefix:
          type: string
        scene_yaml:
          type: string
      required: [asset_id]
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

# Keep local catalogue hash/caps in sync when present.
CATALOGUE="${ROOT}/share/modules/catalogue.yaml"
if [[ -f "$CATALOGUE" ]]; then
  echo "== update catalogue.yaml illustration-studio hash =="
  perl -i -0pe "s/(  - name: illustration-studio\r?\n(?:(?!  - name: ).)*?    hash: )sha256:[a-f0-9]+/\${1}sha256:${HASH}/s" "$CATALOGUE"
  perl -i -0pe "s/(  - name: illustration-studio\r?\n    version: )\"[^\"]+\"/\${1}\"0.7.16\"/" "$CATALOGUE"
  echo "== refresh catalogue signature (UPDATE_CATALOGUE=1) =="
  (cd "${ROOT}" && UPDATE_CATALOGUE=1 cargo test -p aos-platform --no-default-features \
    committed_catalogue_signature_matches -- --nocapture) \
    || echo "WARN: catalogue signature refresh failed — run UPDATE_CATALOGUE=1 cargo test -p aos-platform --no-default-features committed_catalogue_signature_matches"
fi
