#!/usr/bin/env bash
# build-tasks.sh — build and package the tasks WASM module (.aospkg)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../" && pwd)"
TASKS_DIR="${ROOT}/modules/tasks"
STAGING="${ROOT}/modules/tasks.aospkg"
SHARE="${ROOT}/share/modules/tasks.aospkg"
UI_SRC="${TASKS_DIR}/ui/index.html"

echo "== build wasm32 (tasks) =="
TASKS_TARGET="${CARGO_TARGET_DIR:-${ROOT}/target}"
CARGO_TARGET_DIR="${TASKS_TARGET}" \
  cargo build --manifest-path "${TASKS_DIR}/Cargo.toml" \
  --target wasm32-unknown-unknown --release

WASM_SRC=""
for cand in \
  "${TASKS_TARGET}/wasm32-unknown-unknown/release/module_tasks.wasm" \
  "${ROOT}/target/wasm32-unknown-unknown/release/module_tasks.wasm" \
  "${TASKS_DIR}/target/wasm32-unknown-unknown/release/module_tasks.wasm"
do
  if [[ -f "$cand" ]]; then
    WASM_SRC="$cand"
    break
  fi
done
if [[ -z "$WASM_SRC" ]]; then
  echo "ERROR: module_tasks.wasm not found" >&2
  exit 1
fi

if [[ ! -f "$UI_SRC" ]]; then
  echo "ERROR: declarative UI source missing: $UI_SRC" >&2
  exit 1
fi

mkdir -p "${STAGING}/ui" "${STAGING}/schemas"
cp -f "$WASM_SRC" "${STAGING}/module.wasm"
cp -f "$UI_SRC" "${STAGING}/ui/index.html"

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

cat > "${STAGING}/manifest.yaml" <<EOF
name: tasks
version: 1.0.0
hash: ${HASH}
permissions:
  required_caps:
    - fs.read:/documents/tasks/**
    - fs.write:/documents/tasks/**
tools:
  - name: tasks.create
    description: Create a task
    input_schema:
      type: object
      properties:
        title: { type: string }
        notes: { type: string }
      required: [title]
  - name: tasks.list
    description: List tasks
    input_schema:
      type: object
  - name: tasks.update
    description: Update a task
    input_schema:
      type: object
      properties:
        id: { type: string }
        title: { type: string }
        notes: { type: string }
        done: { type: boolean }
      required: [id]
  - name: tasks.complete
    description: Mark a task complete (or reopen)
    input_schema:
      type: object
      properties:
        id: { type: string }
        done: { type: boolean }
      required: [id]
ui:
  entry: ui/index.html
  mode: declarative_ui
min_os_api: 1
EOF

rm -rf "$SHARE"
cp -a "${STAGING}" "$SHARE"
echo "== package ready: ${STAGING} / ${SHARE} (hash ${HASH}) =="

CATALOGUE="${ROOT}/share/modules/catalogue.yaml"
if [[ -f "$CATALOGUE" ]] && grep -q "name: tasks" "$CATALOGUE"; then
  echo "== update catalogue.yaml tasks hash =="
  perl -i -0pe "s/(  - name: tasks\n(?:    .*\n)*?    hash: )sha256:[a-f0-9]+/\${1}sha256:${HASH}/" "$CATALOGUE"
  if command -v cargo >/dev/null 2>&1; then
    (cd "${ROOT}" && UPDATE_CATALOGUE=1 cargo test -p aos-platform --no-default-features \
      catalogue::tests::committed_catalogue_signature_matches -- --nocapture) \
      || echo "WARN: catalogue signature refresh failed"
  fi
fi
