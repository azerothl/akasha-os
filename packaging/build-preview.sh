#!/usr/bin/env bash
# build-preview.sh — assemble Akasha OS Preview (Linux x64 + CUDA)
# GGUF optionnels (SKIP_MODELS=1) — téléchargés au premier run.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="${VERSION:-$(tr -d '[:space:]' < "${ROOT}/VERSION" 2>/dev/null || echo 0.7.0)}"
CPU_ONLY="${CPU_ONLY:-0}"
if [ "$CPU_ONLY" = "1" ]; then
  OUT="${OUT:-${ROOT}/dist/AgentOS-Preview-${VERSION}-linux-x64-cpu}"
else
  OUT="${OUT:-${ROOT}/dist/AgentOS-Preview-${VERSION}-linux-x64}"
fi
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-${ROOT}/target}"

SKIP_BUILD="${SKIP_BUILD:-0}"
SKIP_MODELS="${SKIP_MODELS:-0}"
REQUIRE_CUDA="${REQUIRE_CUDA:-0}"

if [ "$SKIP_BUILD" != "1" ]; then
  # Separate cargo resolve so aos-auditd does not feature-unify llama/CUDA into
  # the audit binary (GitHub Release 2 GiB limit).
  echo "== cargo build --release (aos-auditd sans CUDA/llama) =="
  cargo build --release -p aos-auditd
  if [ "$CPU_ONLY" = "1" ]; then
    echo "== cargo build --release (CPU-only) =="
    cargo build --release -p aos-session -p aos-ipc -p aos-agent \
      -p aos-capkd -p aos-ui-egui
    cargo build --release -p aos-model --no-default-features
    cargo build --release -p aos-platform --no-default-features --features embeddings
  else
    echo "== cargo build --release =="
    cargo build --release -p aos-session -p aos-ipc -p aos-model -p aos-agent \
      -p aos-capkd -p aos-ui-egui
    # Build the preview's platform daemon without optional embeddings so Linux
    # release assets keep a single CUDA/llama-linked binary (aos-modeld) and stay
    # under GitHub's 2 GiB artifact limit.
    echo "== cargo build --release (aos-platformd sans embeddings) =="
    cargo build --release -p aos-platform --bin aos-platformd --no-default-features
  fi
  echo "== notes module =="
  if command -v pwsh >/dev/null 2>&1; then
    pwsh -NoProfile -File "${ROOT}/modules/build-notes.ps1"
  else
    # Clear host-only linker flags for wasm (rust-lld rejects -fuse-ld=lld).
    env -u RUSTFLAGS -u CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS \
      cargo build --manifest-path "${ROOT}/modules/notes/Cargo.toml" \
      --target wasm32-unknown-unknown --release
    mkdir -p "${ROOT}/modules/notes.aospkg/ui" "${ROOT}/modules/notes.aospkg/schemas"
    WASM_SRC=""
    for cand in \
      "${CARGO_TARGET_DIR}/wasm32-unknown-unknown/release/module_notes.wasm" \
      "${ROOT}/target/wasm32-unknown-unknown/release/module_notes.wasm" \
      "${ROOT}/modules/notes/target/wasm32-unknown-unknown/release/module_notes.wasm"
    do
      if [ -f "${cand}" ]; then WASM_SRC="${cand}"; break; fi
    done
    if [ -z "${WASM_SRC}" ]; then
      echo "ERROR: module_notes.wasm introuvable" >&2
      find "${ROOT}" -name 'module_notes.wasm' 2>/dev/null | head -n 20 >&2 || true
      exit 1
    fi
    cp -f "${WASM_SRC}" "${ROOT}/modules/notes.aospkg/module.wasm"
    echo "  wasm: ${WASM_SRC}"
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
  echo "== tasks module =="
  if command -v pwsh >/dev/null 2>&1; then
    pwsh -NoProfile -File "${ROOT}/modules/build-tasks.ps1"
  else
    env -u RUSTFLAGS -u CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS \
      cargo build --manifest-path "${ROOT}/modules/tasks/Cargo.toml" \
      --target wasm32-unknown-unknown --release
    mkdir -p "${ROOT}/modules/tasks.aospkg/ui" "${ROOT}/modules/tasks.aospkg/schemas"
    WASM_TASK=""
    for cand in \
      "${CARGO_TARGET_DIR}/wasm32-unknown-unknown/release/module_tasks.wasm" \
      "${ROOT}/target/wasm32-unknown-unknown/release/module_tasks.wasm" \
      "${ROOT}/modules/tasks/target/wasm32-unknown-unknown/release/module_tasks.wasm"
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
  fi
  if [ -f "${ROOT}/modules/build-ext-rt.ps1" ] && command -v pwsh >/dev/null 2>&1; then
    echo "== ext-rt module =="
    pwsh -NoProfile -File "${ROOT}/modules/build-ext-rt.ps1" || true
  elif [ -f "${ROOT}/modules/ext-rt/Cargo.toml" ]; then
    echo "== ext-rt wasm =="
    env -u RUSTFLAGS -u CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS \
      cargo build --manifest-path "${ROOT}/modules/ext-rt/Cargo.toml" \
      --target wasm32-unknown-unknown --release || true
    WASM_EXT=""
    for cand in \
      "${CARGO_TARGET_DIR}/wasm32-unknown-unknown/release/module_ext_rt.wasm" \
      "${ROOT}/target/wasm32-unknown-unknown/release/module_ext_rt.wasm" \
      "${ROOT}/modules/ext-rt/target/wasm32-unknown-unknown/release/module_ext_rt.wasm"
    do
      if [ -f "${cand}" ]; then WASM_EXT="${cand}"; break; fi
    done
    if [ -n "${WASM_EXT}" ]; then
      mkdir -p "${ROOT}/share/modules/ext-rt.aospkg/ui" "${ROOT}/share/modules/ext-rt.aospkg/assets"
      cp -f "${WASM_EXT}" "${ROOT}/share/modules/ext-rt.aospkg/module.wasm"
      echo "  wasm: ${WASM_EXT}"
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
if command -v strip >/dev/null 2>&1; then
  echo "== strip release binaries =="
  strip --strip-unneeded "${OUT}/bin/"aos-* 2>/dev/null || strip "${OUT}/bin/"aos-* || true
fi

# CUDA runtime .so next to binaries (skip for CPU-only artefacts)
cuda_copied=0
if [ "$CPU_ONLY" != "1" ]; then
CUDA_HOME="${CUDA_HOME:-${CUDA_PATH:-/usr/local/cuda}}"
if command -v readlink >/dev/null 2>&1; then
  CUDA_HOME_REAL="$(readlink -f "${CUDA_HOME}" 2>/dev/null || true)"
else
  CUDA_HOME_REAL=""
fi
CUDA_LIB_CANDIDATES=(
  "${CUDA_HOME}"
  "${CUDA_HOME}/lib64"
  "${CUDA_HOME}/lib"
  "${CUDA_HOME}/targets/x86_64-linux/lib"
)
if [ -n "${CUDA_HOME_REAL}" ]; then
  CUDA_LIB_CANDIDATES+=(
    "${CUDA_HOME_REAL}"
    "${CUDA_HOME_REAL}/lib64"
    "${CUDA_HOME_REAL}/lib"
    "${CUDA_HOME_REAL}/targets/x86_64-linux/lib"
  )
fi
CUDA_LIB_CANDIDATES+=(
  /usr/local/cuda
  /usr/local/cuda/lib64
  /usr/local/cuda/lib
  /usr/local/cuda/targets/x86_64-linux/lib
  /usr/lib/x86_64-linux-gnu
)
# Map realpath -> canonical basename already placed in ${OUT}/bin (dedupe SONAME
# aliases: libcublasLt.so / .so.12 / .so.12.4.x used to be copied 3× with cp -L
# and blew past GitHub's 2 GiB release asset limit).
declare -A CUDA_REAL_TO_CANON=()
echo "== CUDA runtime (CUDA_HOME=${CUDA_HOME}) =="

copy_cuda_from_dir() {
  local dir="$1"
  local f base real canon
  [ -d "${dir}" ] || return 0
  case "${dir}" in
    */stubs) return 0 ;;
  esac
  # Globs must be expanded against ${dir}, never CWD (nullglob + libcudart.so* in
  # `for pat in` would wipe the pattern list when the repo root has no .so).
  local files=()
  shopt -s nullglob
  files=(
    "${dir}"/libcudart.so*
    "${dir}"/libcublas.so*
    "${dir}"/libcublasLt.so*
    "${dir}"/libnvJitLink.so*
    "${dir}"/libnvrtc.so*
    "${dir}"/libnvrtc-builtins.so*
  )
  shopt -u nullglob
  for f in "${files[@]}"; do
    [ -e "${f}" ] || continue
    case "${f}" in
      */stubs/*) continue ;;
    esac
    base="$(basename "${f}")"
    if command -v readlink >/dev/null 2>&1; then
      real="$(readlink -f "${f}" 2>/dev/null || true)"
    else
      real=""
    fi
    if [ -z "${real}" ] || [ ! -e "${real}" ]; then
      real="${f}"
    fi
    canon="${CUDA_REAL_TO_CANON[${real}]:-}"
    if [ -z "${canon}" ]; then
      canon="$(basename "${real}")"
      if [ ! -e "${OUT}/bin/${canon}" ]; then
        if ! cp -L "${real}" "${OUT}/bin/${canon}"; then
          echo "  WARN: cp failed ${real} -> ${OUT}/bin/${canon}" >&2
          continue
        fi
        echo "  + ${canon}  (depuis ${dir})"
        cuda_copied=$((cuda_copied + 1))
      fi
      CUDA_REAL_TO_CANON["${real}"]="${canon}"
    fi
    if [ "${base}" != "${canon}" ] && [ ! -e "${OUT}/bin/${base}" ]; then
      ln -sfn "${canon}" "${OUT}/bin/${base}"
      echo "  ~ ${base} -> ${canon}"
    fi
  done
}

for CUDA_LIB in "${CUDA_LIB_CANDIDATES[@]}"; do
  copy_cuda_from_dir "${CUDA_LIB}"
done
if [ "${cuda_copied}" -eq 0 ] && command -v ldconfig >/dev/null 2>&1; then
  echo "  fallback ldconfig…"
  while IFS= read -r so; do
    [ -n "${so}" ] || continue
    copy_cuda_from_dir "$(dirname "${so}")"
  done < <(ldconfig -p 2>/dev/null | awk '/libcudart\.so/ { print $NF }' || true)
fi
if [ "${cuda_copied}" -eq 0 ] && [ -d "${CUDA_HOME}" ] && command -v find >/dev/null 2>&1; then
  echo "  fallback find sous ${CUDA_HOME}…"
  while IFS= read -r f; do
    [ -n "${f}" ] || continue
    copy_cuda_from_dir "$(dirname "${f}")"
  done < <(find -L "${CUDA_HOME}" \( -type f -o -type l \) -name 'libcudart.so*' \
    ! -path '*/stubs/*' 2>/dev/null | head -n 40 || true)
fi
if [ "${cuda_copied}" -eq 0 ]; then
  msg="CUDA runtime .so introuvables"
  echo "DEBUG: listing CUDA_HOME=${CUDA_HOME}" >&2
  ls -la "${CUDA_HOME}" 2>/dev/null >&2 || true
  ls -la "${CUDA_HOME}/lib64" 2>/dev/null >&2 || true
  ls -la "${CUDA_HOME}/targets/x86_64-linux/lib" 2>/dev/null | head -n 40 >&2 || true
  find -L "${CUDA_HOME}" \( -type f -o -type l \) -name 'libcudart.so*' 2>/dev/null | head -n 20 >&2 || true
  if [ "${REQUIRE_CUDA}" = "1" ]; then echo "ERROR: ${msg}" >&2; exit 1; else echo "WARN: ${msg}" >&2; fi
fi
else
  echo "== CPU-only package — skipping CUDA runtime copy =="
fi

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

if [ -d "${ROOT}/share/modules/notes.aospkg" ]; then
  rm -rf "${OUT}/share/modules/notes.aospkg"
  cp -a "${ROOT}/share/modules/notes.aospkg" "${OUT}/share/modules/notes.aospkg"
elif [ -d "${ROOT}/modules/notes.aospkg" ]; then
  rm -rf "${OUT}/share/modules/notes.aospkg"
  cp -a "${ROOT}/modules/notes.aospkg" "${OUT}/share/modules/notes.aospkg"
fi
if [ -d "${ROOT}/share/modules/tasks.aospkg" ]; then
  rm -rf "${OUT}/share/modules/tasks.aospkg"
  cp -a "${ROOT}/share/modules/tasks.aospkg" "${OUT}/share/modules/tasks.aospkg"
elif [ -d "${ROOT}/modules/tasks.aospkg" ]; then
  rm -rf "${OUT}/share/modules/tasks.aospkg"
  cp -a "${ROOT}/modules/tasks.aospkg" "${OUT}/share/modules/tasks.aospkg"
fi
if [ -d "${ROOT}/share/modules/ext-rt.aospkg" ]; then
  rm -rf "${OUT}/share/modules/ext-rt.aospkg"
  cp -a "${ROOT}/share/modules/ext-rt.aospkg" "${OUT}/share/modules/ext-rt.aospkg"
elif [ -d "${ROOT}/modules/ext-rt.aospkg" ]; then
  rm -rf "${OUT}/share/modules/ext-rt.aospkg"
  cp -a "${ROOT}/modules/ext-rt.aospkg" "${OUT}/share/modules/ext-rt.aospkg"
fi

for cat in catalogue.yaml catalogue.yaml.sig catalogue.pub; do
  src="${ROOT}/share/modules/${cat}"
  if [ ! -f "${src}" ]; then
    echo "ERROR: missing ${src} (E10 catalogue)" >&2
    exit 1
  fi
  cp -f "${src}" "${OUT}/share/modules/${cat}"
done

if [ -d "${ROOT}/skills" ]; then
  cp -a "${ROOT}/skills/." "${OUT}/share/skills/"
fi

mkdir -p "${OUT}/share/mcp"
if [ -f "${ROOT}/share/mcp/servers.yaml.example" ]; then
  cp -f "${ROOT}/share/mcp/servers.yaml.example" "${OUT}/share/mcp/servers.yaml.example"
elif [ -f "${ROOT}/var/mcp/servers.yaml.example" ]; then
  cp -f "${ROOT}/var/mcp/servers.yaml.example" "${OUT}/share/mcp/servers.yaml.example"
else
  cat > "${OUT}/share/mcp/servers.yaml.example" <<'EOF'
# MCP servers (stdio). Copy to var/mcp/servers.yaml and adapt.
# Use ${secret:name} for vault-backed API keys (Settings → Secrets).
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

cp -f "${ROOT}/INSTALL.md" "${OUT}/" 2>/dev/null || true
cp -f "${ROOT}/docs/TESTER.md" "${OUT}/TESTER.md" 2>/dev/null || true
cp -f "${ROOT}/docs/FIRST-RUN.md" "${OUT}/FIRST-RUN.md" 2>/dev/null || true
mkdir -p "${OUT}/docs"
cp -f "${ROOT}/docs/FIRST-RUN.md" "${OUT}/docs/FIRST-RUN.md" 2>/dev/null || true
cp -f "${ROOT}/docs/STATUS.md" "${OUT}/docs/STATUS.md" 2>/dev/null || true
cp -f "${ROOT}/docs/FEATURES.md" "${OUT}/docs/FEATURES.md" 2>/dev/null || true
cp -f "${ROOT}/docs/I18N.md" "${OUT}/docs/I18N.md" 2>/dev/null || true
if [ -d "${ROOT}/docs/fr" ]; then
  mkdir -p "${OUT}/docs/fr"
  cp -a "${ROOT}/docs/fr/." "${OUT}/docs/fr/"
fi
cp -f "${ROOT}/LICENSE" "${OUT}/" 2>/dev/null || true
cp -f "${ROOT}/NOTICE" "${OUT}/" 2>/dev/null || true
cp -f "${ROOT}/LICENSE-COMMERCIAL.md" "${OUT}/" 2>/dev/null || true
cp -f "$(dirname "$0")/install-linux.sh" "${OUT}/install.sh"
chmod +x "${OUT}/bin/"* "${OUT}/install.sh"

cat > "${OUT}/README.txt" <<EOF
Akasha OS Preview ${VERSION} (Linux x64 + NVIDIA)

1. Prérequis : driver NVIDIA, nvidia-smi OK, ~4 Go disque
2. ./install.sh
   Données stables : ~/.local/share/agentos-preview (sessions, mémoire, notes).
   Lancer bin/aos-session depuis cette archive synchronise aussi vers ce préfixe.
3. Premier lancement : télécharge les modèles si besoin, puis le tutoriel
4. Agents agentic : skills (share/skills), MCP (var/mcp/servers.yaml), sous-agents
5. Voir FIRST-RUN.md, INSTALL.md, TESTER.md, docs/FEATURES.md (et docs/fr/ pour le français)
EOF

echo "== package prêt : ${OUT} =="
du -sh "${OUT}"
echo "== plus gros fichiers =="
du -ah "${OUT}" 2>/dev/null | sort -hr | head -n 25 || true
# Fail early if the tree alone already exceeds GitHub Release asset limit (~2 GiB),
# even before tar/gzip (archives of CUDA libs barely shrink).
MAX_BYTES=$((2 * 1024 * 1024 * 1024 - 1))
TREE_BYTES="$(du -sb "${OUT}" | awk '{print $1}')"
echo "taille arbre: ${TREE_BYTES} bytes (limite release ${MAX_BYTES})"
if [ "${TREE_BYTES}" -ge "${MAX_BYTES}" ]; then
  echo "ERROR: package ${OUT} exceeds GitHub Release 2 GiB asset limit" >&2
  exit 1
fi
