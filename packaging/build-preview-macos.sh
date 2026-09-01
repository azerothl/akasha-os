#!/usr/bin/env bash
# build-preview-macos.sh — assemble Akasha OS Preview (macOS Apple Silicon + Metal)
# Must run on an arm64 Mac (Apple Silicon). Intel Mac is not supported.
# GGUF optional (SKIP_MODELS=1) — downloaded on first run.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="${VERSION:-$(tr -d '[:space:]' < "${ROOT}/VERSION" 2>/dev/null || echo 0.15.1)}"
OUT="${OUT:-${ROOT}/dist/AgentOS-Preview-${VERSION}-macos-arm64}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-${ROOT}/target}"

SKIP_BUILD="${SKIP_BUILD:-0}"
SKIP_MODELS="${SKIP_MODELS:-0}"

if [ "$(uname -s)" != "Darwin" ]; then
  echo "ERROR: build-preview-macos.sh must run on macOS" >&2
  exit 1
fi
if [ "$(uname -m)" != "arm64" ]; then
  echo "ERROR: Apple Silicon (arm64) required — Intel Mac is not a Preview target" >&2
  exit 1
fi

# GNU du -sb is unavailable on stock macOS (EX_USAGE / exit 64).
dir_size_bytes() {
  local d="$1"
  if du -sb "$d" >/dev/null 2>&1; then
    du -sb "$d" | awk '{print $1}'
  else
    du -sk "$d" | awk '{print $1 * 1024}'
  fi
}

copy_tree() {
  local src="$1"
  local dst="$2"
  echo "cp -a ${src} ${dst}"
  cp -a "$src" "$dst"
}

if [ "$SKIP_BUILD" != "1" ]; then
  echo "== cargo build --release (aos-auditd sans llama) =="
  cargo build --release -p aos-auditd

  echo "== cargo build --release (Metal) =="
  cargo build --release -p aos-session -p aos-ipc -p aos-agent \
    -p aos-capkd -p aos-ui-egui -p aos-bridge
  cargo build --release -p aos-model --no-default-features --features metal
  cargo build --release -p aos-platform --no-default-features --features embeddings,metal

  echo "== notes module =="
  if [ -f "${ROOT}/modules/build-canvas.sh" ]; then
    echo "== canvas module =="
    "${ROOT}/modules/build-canvas.sh"
  fi
  env -u RUSTFLAGS \
    cargo build --manifest-path "${ROOT}/modules/notes/Cargo.toml" \
    --target wasm32-unknown-unknown --release
  mkdir -p "${ROOT}/modules/notes.aospkg/ui" "${ROOT}/modules/notes.aospkg/schemas"
  WASM_SRC=""
  for cand in \
    "${CARGO_TARGET_DIR}/wasm32-unknown-unknown/release/module_notes.wasm" \
    "${ROOT}/target/wasm32-unknown-unknown/release/module_notes.wasm"
  do
    if [ -f "${cand}" ]; then WASM_SRC="${cand}"; break; fi
  done
  if [ -z "${WASM_SRC}" ]; then
    echo "ERROR: module_notes.wasm introuvable" >&2
    exit 1
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

  echo "== tasks module =="
  env -u RUSTFLAGS \
    cargo build --manifest-path "${ROOT}/modules/tasks/Cargo.toml" \
    --target wasm32-unknown-unknown --release
  mkdir -p "${ROOT}/modules/tasks.aospkg/ui"
  WASM_TASK=""
  for cand in \
    "${CARGO_TARGET_DIR}/wasm32-unknown-unknown/release/module_tasks.wasm" \
    "${ROOT}/target/wasm32-unknown-unknown/release/module_tasks.wasm"
  do
    if [ -f "${cand}" ]; then WASM_TASK="${cand}"; break; fi
  done
  if [ -z "${WASM_TASK}" ]; then
    echo "ERROR: module_tasks.wasm introuvable" >&2
    exit 1
  fi
  cp -f "${WASM_TASK}" "${ROOT}/modules/tasks.aospkg/module.wasm"
  cat > "${ROOT}/modules/tasks.aospkg/manifest.yaml" <<'EOF'
name: tasks
version: 1.0.0
hash: "ci"
permissions:
  required_caps: []
tools: []
ui:
  entry: ui/index.html
  mode: declarative_ui
min_os_api: 1
EOF
  echo '{"type":"declarative_ui","title":"Tasks","commands":["tasks.create","tasks.list","tasks.update","tasks.complete"]}' \
    > "${ROOT}/modules/tasks.aospkg/ui/index.html"

  if [ -f "${ROOT}/modules/ext-rt/Cargo.toml" ]; then
    echo "== ext-rt wasm =="
    env -u RUSTFLAGS \
      cargo build --manifest-path "${ROOT}/modules/ext-rt/Cargo.toml" \
      --target wasm32-unknown-unknown --release || true
    WASM_EXT=""
    for cand in \
      "${CARGO_TARGET_DIR}/wasm32-unknown-unknown/release/module_ext_rt.wasm" \
      "${ROOT}/target/wasm32-unknown-unknown/release/module_ext_rt.wasm"
    do
      if [ -f "${cand}" ]; then WASM_EXT="${cand}"; break; fi
    done
    if [ -n "${WASM_EXT}" ]; then
      mkdir -p "${ROOT}/share/modules/ext-rt.aospkg/ui" "${ROOT}/share/modules/ext-rt.aospkg/assets"
      cp -f "${WASM_EXT}" "${ROOT}/share/modules/ext-rt.aospkg/module.wasm"
    fi
  fi
fi

mkdir -p "${OUT}/bin" "${OUT}/etc" "${OUT}/share/models" \
  "${OUT}/share/models/lora" "${OUT}/share/models/vae" "${OUT}/share/models/styles" "${OUT}/share/models/upscale" \
  "${OUT}/share/modules" "${OUT}/share/skills" \
  "${OUT}/data/models" "${OUT}/var" "${OUT}/docs"

PREVIEW_BINS=(
  aos-session aos-busd aos-modeld aos-agentd aos-agent-worker
  aos-platformd aos-capkd aos-auditd aos-ui-egui aos-bridged
)

for b in "${PREVIEW_BINS[@]}"; do
  src="${CARGO_TARGET_DIR}/release/${b}"
  if [ ! -f "${src}" ]; then
    echo "ERROR: missing release binary ${src} (cargo build aos-ipc for aos-busd?)" >&2
    exit 1
  fi
  echo "cp -f ${src} ${OUT}/bin/"
  cp -f "${src}" "${OUT}/bin/"
done

echo "== cargo build --release (aos-modeld-cpu, no Metal) =="
cargo build --release -p aos-model --no-default-features
cpu_src="${CARGO_TARGET_DIR}/release/aos-modeld"
if [ ! -f "${cpu_src}" ]; then
  echo "ERROR: missing ${cpu_src} after CPU modeld build" >&2
  exit 1
fi
echo "cp -f ${cpu_src} ${OUT}/bin/aos-modeld-cpu"
cp -f "${cpu_src}" "${OUT}/bin/aos-modeld-cpu"
chmod +x "${OUT}/bin/aos-modeld-cpu"

cp -f "${ROOT}/data/models/catalog.yaml" "${OUT}/data/models/"
cp -f "${ROOT}/VERSION" "${OUT}/VERSION"
cp -f "${ROOT}/share/models/manifest.json" "${OUT}/share/models/manifest.json"
if [ -d "${ROOT}/share/icons" ]; then
  mkdir -p "${OUT}/share/icons"
  cp -f "${ROOT}/share/icons/"* "${OUT}/share/icons/"
fi
if [ -f "${ROOT}/share/models/catalog-offerings.json" ]; then
  cp -f "${ROOT}/share/models/catalog-offerings.json" "${OUT}/share/models/catalog-offerings.json"
fi

for pkg in notes tasks ext-rt canvas; do
  for base in "${ROOT}/share/modules/${pkg}.aospkg" "${ROOT}/modules/${pkg}.aospkg"; do
    if [ -d "${base}" ]; then
      rm -rf "${OUT}/share/modules/${pkg}.aospkg"
      copy_tree "${base}" "${OUT}/share/modules/${pkg}.aospkg"
      break
    fi
  done
done

for cat in catalogue.yaml catalogue.yaml.sig catalogue.pub; do
  src="${ROOT}/share/modules/${cat}"
  if [ ! -f "${src}" ]; then
    echo "ERROR: missing ${src} (E10 catalogue)" >&2
    exit 1
  fi
  cp -f "${src}" "${OUT}/share/modules/${cat}"
done

if [ -d "${ROOT}/skills" ]; then
  copy_tree "${ROOT}/skills/." "${OUT}/share/skills/"
fi

mkdir -p "${OUT}/share/mcp"
if [ -f "${ROOT}/share/mcp/servers.yaml.example" ]; then
  cp -f "${ROOT}/share/mcp/servers.yaml.example" "${OUT}/share/mcp/servers.yaml.example"
else
  cat > "${OUT}/share/mcp/servers.yaml.example" <<'EOF'
# MCP servers (stdio). Copy to var/mcp/servers.yaml and adapt.
servers: {}
EOF
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

cp -f "${ROOT}/docs/INSTALL.md" "${OUT}/INSTALL.md" 2>/dev/null || true
cp -f "${ROOT}/docs/TESTER.md" "${OUT}/TESTER.md" 2>/dev/null || true
cp -f "${ROOT}/docs/FIRST-RUN.md" "${OUT}/FIRST-RUN.md" 2>/dev/null || true
mkdir -p "${OUT}/docs"
cp -f "${ROOT}/docs/INSTALL.md" "${OUT}/docs/INSTALL.md" 2>/dev/null || true
cp -f "${ROOT}/docs/FIRST-RUN.md" "${OUT}/docs/FIRST-RUN.md" 2>/dev/null || true
cp -f "${ROOT}/docs/STATUS.md" "${OUT}/docs/STATUS.md" 2>/dev/null || true
cp -f "${ROOT}/docs/FEATURES.md" "${OUT}/docs/FEATURES.md" 2>/dev/null || true
cp -f "${ROOT}/docs/I18N.md" "${OUT}/docs/I18N.md" 2>/dev/null || true
cp -f "${ROOT}/docs/write-a-skill.md" "${OUT}/docs/write-a-skill.md" 2>/dev/null || true
if [ -d "${ROOT}/docs/fr" ]; then
  mkdir -p "${OUT}/docs/fr"
  copy_tree "${ROOT}/docs/fr/." "${OUT}/docs/fr/"
fi
cp -f "${ROOT}/LICENSE" "${OUT}/" 2>/dev/null || true
cp -f "${ROOT}/LICENSE-APACHE" "${OUT}/" 2>/dev/null || true
cp -f "${ROOT}/LICENSE-MIT" "${OUT}/" 2>/dev/null || true
cp -f "${ROOT}/NOTICE" "${OUT}/" 2>/dev/null || true
cp -f "${ROOT}/LICENSE-COMMERCIAL.md" "${OUT}/" 2>/dev/null || true
cp -f "$(dirname "$0")/install-macos.sh" "${OUT}/install.sh"
echo "chmod +x ${OUT}/bin/* ${OUT}/install.sh"
chmod +x "${OUT}/bin/"* "${OUT}/install.sh"

cat > "${OUT}/README.txt" <<EOF
Akasha OS Preview ${VERSION} (macOS Apple Silicon + Metal)

1. Prérequis : Mac Apple Silicon (arm64), ~8 Go disque libre
2. ./install.sh
   Données stables : ~/.local/share/agentos-preview
   Lancer bin/aos-session depuis cette archive synchronise aussi vers ce préfixe.
3. Premier lancement : télécharge les modèles si besoin, puis le tutoriel
4. Agents agentic : skills (share/skills), MCP (var/mcp/servers.yaml), sous-agents
5. Voir FIRST-RUN.md, INSTALL.md, TESTER.md, docs/FEATURES.md (et docs/fr/ pour le français)

Build non signé : si macOS bloque l'ouverture, install.sh exécute xattr -cr sur bin/.
EOF

echo "== package prêt : ${OUT} =="
du -sh "${OUT}"
MAX_BYTES=$((2 * 1024 * 1024 * 1024 - 1))
TREE_BYTES="$(dir_size_bytes "${OUT}")"
echo "taille arbre: ${TREE_BYTES} bytes (limite release ${MAX_BYTES})"
if [ "${TREE_BYTES}" -ge "${MAX_BYTES}" ]; then
  echo "ERROR: package exceeds GitHub Release 2 GiB asset limit" >&2
  exit 1
fi

ZIP_PATH="${ROOT}/dist/AgentOS-Preview-${VERSION}-macos-arm64.zip"
if [ ! -d "${OUT}" ]; then
  echo "ERROR: package directory missing: ${OUT}" >&2
  exit 1
fi
rm -f "${ZIP_PATH}"
echo "== ditto -c -k --keepParent ${OUT} ${ZIP_PATH} =="
ditto -c -k --keepParent "${OUT}" "${ZIP_PATH}"
ls -lh "${ZIP_PATH}"
