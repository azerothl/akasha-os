#!/usr/bin/env bash
# build-preview.sh — assemble Agent OS Preview 0.1 (Linux x64 + CUDA)
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${OUT:-${ROOT}/dist/AgentOS-Preview-0.1-linux-x64}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-${ROOT}/target}"

SKIP_BUILD="${SKIP_BUILD:-0}"
SKIP_MODELS="${SKIP_MODELS:-0}"

if [ "$SKIP_BUILD" != "1" ]; then
  echo "== cargo build --release =="
  cargo build --release -p aos-session -p aos-ipc -p aos-model -p aos-agent \
    -p aos-platform -p aos-capkd -p aos-auditd -p aos-ui-egui
  echo "== notes module =="
  if command -v pwsh >/dev/null 2>&1; then
    pwsh -NoProfile -File "${ROOT}/modules/build-notes.ps1"
  else
    cargo build --manifest-path "${ROOT}/modules/notes/Cargo.toml" \
      --target wasm32-unknown-unknown --release
    mkdir -p "${ROOT}/modules/notes.aospkg/ui"
    cp "${ROOT}/modules/notes/target/wasm32-unknown-unknown/release/module_notes.wasm" \
      "${ROOT}/modules/notes.aospkg/module.wasm"
  fi
fi

mkdir -p "${OUT}/bin" "${OUT}/etc" "${OUT}/share/models" \
  "${OUT}/share/modules" "${OUT}/data/models" "${OUT}/var"

for b in aos-session aos-busd aos-modeld aos-agentd aos-agent-worker \
         aos-platformd aos-capkd aos-auditd aos-ui-egui; do
  cp -f "${CARGO_TARGET_DIR}/release/${b}" "${OUT}/bin/"
done

cp -f "${ROOT}/data/models/catalog.yaml" "${OUT}/data/models/"
if [ -d "${ROOT}/modules/notes.aospkg" ]; then
  rm -rf "${OUT}/share/modules/notes.aospkg"
  cp -a "${ROOT}/modules/notes.aospkg" "${OUT}/share/modules/notes.aospkg"
fi

if [ "$SKIP_MODELS" != "1" ]; then
  for m in qwen2.5-3b-instruct-q4_k_m.gguf qwen2.5-0.5b-instruct-q4_k_m.gguf; do
    if [ -f "${ROOT}/tools/models/${m}" ]; then
      cp -f "${ROOT}/tools/models/${m}" "${OUT}/share/models/"
    else
      echo "WARN: GGUF manquant tools/models/${m}" >&2
    fi
  done
fi

cp -f "${ROOT}/INSTALL.md" "${OUT}/" 2>/dev/null || true
cp -f "${ROOT}/docs/TESTER.md" "${OUT}/TESTER.md" 2>/dev/null || true
cp -f "$(dirname "$0")/install-linux.sh" "${OUT}/install.sh"
chmod +x "${OUT}/bin/"* "${OUT}/install.sh"

cat > "${OUT}/README.txt" <<EOF
Agent OS Preview 0.1 (Linux x64 + NVIDIA)

1. Prérequis : driver NVIDIA, nvidia-smi OK
2. ./install.sh   # → ~/.local/share/agentos-preview
3. agentos-preview   # ou ~/.local/bin/agentos-preview

Voir INSTALL.md et TESTER.md.
EOF

echo "== package prêt : ${OUT} =="
du -sh "${OUT}"
