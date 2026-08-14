#!/usr/bin/env bash
# build-preview.sh — assemble Agent OS Preview (Linux x64 + CUDA)
# GGUF optionnels (SKIP_MODELS=1) — téléchargés au premier run.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="${VERSION:-$(tr -d '[:space:]' < "${ROOT}/VERSION" 2>/dev/null || echo 0.1.0)}"
OUT="${OUT:-${ROOT}/dist/AgentOS-Preview-${VERSION}-linux-x64}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-${ROOT}/target}"

SKIP_BUILD="${SKIP_BUILD:-0}"
SKIP_MODELS="${SKIP_MODELS:-0}"
REQUIRE_CUDA="${REQUIRE_CUDA:-0}"

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
    mkdir -p "${ROOT}/modules/notes.aospkg/ui" "${ROOT}/modules/notes.aospkg/schemas"
    WASM_SRC="${CARGO_TARGET_DIR}/wasm32-unknown-unknown/release/module_notes.wasm"
    if [ ! -f "${WASM_SRC}" ]; then
      WASM_SRC="${ROOT}/modules/notes/target/wasm32-unknown-unknown/release/module_notes.wasm"
    fi
    cp -f "${WASM_SRC}" "${ROOT}/modules/notes.aospkg/module.wasm"
    cat > "${ROOT}/modules/notes.aospkg/manifest.yaml" <<'EOF'
name: notes
version: 0.1.0
hash: "ci"
permissions:
  required_caps: []
tools: []
ui:
  entry: ui/index.html
  mode: declarative_ui
min_os_api: 1
EOF
  fi
  if [ -f "${ROOT}/modules/build-ext-rt.ps1" ] && command -v pwsh >/dev/null 2>&1; then
    echo "== ext-rt module =="
    pwsh -NoProfile -File "${ROOT}/modules/build-ext-rt.ps1" || true
  elif [ -f "${ROOT}/modules/ext-rt/Cargo.toml" ]; then
    echo "== ext-rt wasm =="
    cargo build --manifest-path "${ROOT}/modules/ext-rt/Cargo.toml" \
      --target wasm32-unknown-unknown --release || true
    WASM_EXT="${CARGO_TARGET_DIR}/wasm32-unknown-unknown/release/module_ext_rt.wasm"
    if [ -f "${WASM_EXT}" ]; then
      mkdir -p "${ROOT}/share/modules/ext-rt.aospkg/ui" "${ROOT}/share/modules/ext-rt.aospkg/assets"
      cp -f "${WASM_EXT}" "${ROOT}/share/modules/ext-rt.aospkg/module.wasm"
    fi
  fi
fi

mkdir -p "${OUT}/bin" "${OUT}/etc" "${OUT}/share/models" \
  "${OUT}/share/modules" "${OUT}/share/skills" \
  "${OUT}/data/models" "${OUT}/var" "${OUT}/docs"

for b in aos-session aos-busd aos-modeld aos-agentd aos-agent-worker \
         aos-platformd aos-capkd aos-auditd aos-ui-egui; do
  cp -f "${CARGO_TARGET_DIR}/release/${b}" "${OUT}/bin/"
done

# CUDA runtime .so next to binaries
CUDA_HOME="${CUDA_HOME:-${CUDA_PATH:-/usr/local/cuda}}"
CUDA_LIB=""
for cand in "${CUDA_HOME}/lib64" "${CUDA_HOME}/lib" /usr/local/cuda/lib64; do
  if [ -d "${cand}" ]; then CUDA_LIB="${cand}"; break; fi
done
cuda_copied=0
if [ -n "${CUDA_LIB}" ]; then
  echo "== CUDA runtime depuis ${CUDA_LIB} =="
  shopt -s nullglob
  for pat in libcudart.so* libcublas.so* libcublasLt.so* libnvJitLink.so* libnvrtc.so* libnvrtc-builtins.so*; do
    for f in "${CUDA_LIB}"/${pat}; do
      cp -a "${f}" "${OUT}/bin/" 2>/dev/null || true
      echo "  + $(basename "${f}")"
      cuda_copied=$((cuda_copied + 1))
    done
  done
  shopt -u nullglob
fi
if [ "${cuda_copied}" -eq 0 ]; then
  msg="CUDA runtime .so introuvables"
  if [ "${REQUIRE_CUDA}" = "1" ]; then echo "ERROR: ${msg}" >&2; exit 1; else echo "WARN: ${msg}" >&2; fi
fi

cp -f "${ROOT}/data/models/catalog.yaml" "${OUT}/data/models/"
cp -f "${ROOT}/VERSION" "${OUT}/VERSION"
cp -f "${ROOT}/share/models/manifest.json" "${OUT}/share/models/manifest.json"

if [ -d "${ROOT}/modules/notes.aospkg" ]; then
  rm -rf "${OUT}/share/modules/notes.aospkg"
  cp -a "${ROOT}/modules/notes.aospkg" "${OUT}/share/modules/notes.aospkg"
fi
if [ -d "${ROOT}/share/modules/ext-rt.aospkg" ]; then
  rm -rf "${OUT}/share/modules/ext-rt.aospkg"
  cp -a "${ROOT}/share/modules/ext-rt.aospkg" "${OUT}/share/modules/ext-rt.aospkg"
elif [ -d "${ROOT}/modules/ext-rt.aospkg" ]; then
  rm -rf "${OUT}/share/modules/ext-rt.aospkg"
  cp -a "${ROOT}/modules/ext-rt.aospkg" "${OUT}/share/modules/ext-rt.aospkg"
fi

if [ -d "${ROOT}/skills" ]; then
  cp -a "${ROOT}/skills/." "${OUT}/share/skills/"
fi

if [ "$SKIP_MODELS" != "1" ]; then
  for m in qwen2.5-3b-instruct-q4_k_m.gguf qwen2.5-0.5b-instruct-q4_k_m.gguf; do
    if [ -f "${ROOT}/tools/models/${m}" ]; then
      cp -f "${ROOT}/tools/models/${m}" "${OUT}/share/models/"
    else
      echo "WARN: GGUF manquant (OK en CI) tools/models/${m}" >&2
    fi
  done
fi

cp -f "${ROOT}/INSTALL.md" "${OUT}/" 2>/dev/null || true
cp -f "${ROOT}/docs/TESTER.md" "${OUT}/TESTER.md" 2>/dev/null || true
cp -f "${ROOT}/docs/FIRST-RUN.md" "${OUT}/FIRST-RUN.md" 2>/dev/null || true
cp -f "${ROOT}/docs/FIRST-RUN.md" "${OUT}/docs/FIRST-RUN.md" 2>/dev/null || true
cp -f "${ROOT}/LICENSE" "${OUT}/" 2>/dev/null || true
cp -f "${ROOT}/NOTICE" "${OUT}/" 2>/dev/null || true
cp -f "${ROOT}/LICENSE-COMMERCIAL.md" "${OUT}/" 2>/dev/null || true
cp -f "$(dirname "$0")/install-linux.sh" "${OUT}/install.sh"
chmod +x "${OUT}/bin/"* "${OUT}/install.sh"

cat > "${OUT}/README.txt" <<EOF
Agent OS Preview ${VERSION} (Linux x64 + NVIDIA)

1. Prérequis : driver NVIDIA, nvidia-smi OK, ~4 Go disque
2. ./install.sh
3. Premier lancement : télécharge les modèles si besoin, puis le tutoriel
4. Voir FIRST-RUN.md, INSTALL.md, TESTER.md
EOF

echo "== package prêt : ${OUT} =="
du -sh "${OUT}"
