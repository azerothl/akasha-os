#!/usr/bin/env bash
# install-macos.sh — installe / met à jour Akasha OS Preview (Apple Silicon, non destructif)
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
PREFIX="${PREFIX:-${AOS_HOME:-${HOME}/.local/share/agentos-preview}}"
BIN_LINK="${HOME}/.local/bin/agentos-preview"

if [ "$(uname -m)" != "arm64" ]; then
  echo "ERROR: Apple Silicon (arm64) requis — Intel Mac non supporté." >&2
  exit 1
fi

echo "Installation / mise à jour vers ${PREFIX}"
mkdir -p "${PREFIX}" "${HOME}/.local/bin" \
  "${PREFIX}/var" "${PREFIX}/etc" \
  "${PREFIX}/var/mcp" "${PREFIX}/var/skills" "${PREFIX}/var/agents" \
  "${PREFIX}/skills"

for dir in bin share data docs; do
  if [ -d "${HERE}/${dir}" ]; then
    mkdir -p "${PREFIX}/${dir}"
    cp -a "${HERE}/${dir}/." "${PREFIX}/${dir}/"
  fi
done
for f in VERSION INSTALL.md TESTER.md FIRST-RUN.md README.txt \
         LICENSE LICENSE-APACHE LICENSE-MIT NOTICE LICENSE-COMMERCIAL.md install.sh; do
  if [ -f "${HERE}/${f}" ]; then
    cp -f "${HERE}/${f}" "${PREFIX}/"
  fi
done

# Gatekeeper : unsigned Preview build — clear quarantine on shipped binaries.
if command -v xattr >/dev/null 2>&1; then
  xattr -cr "${PREFIX}/bin" 2>/dev/null || true
fi

if [ ! -f "${PREFIX}/var/mcp/servers.yaml" ]; then
  if [ -f "${PREFIX}/share/mcp/servers.yaml.example" ]; then
    cp -f "${PREFIX}/share/mcp/servers.yaml.example" "${PREFIX}/var/mcp/servers.yaml"
  else
    printf '%s\n' '# MCP servers (stdio)' 'servers: {}' > "${PREFIX}/var/mcp/servers.yaml"
  fi
fi

if [ -d "${PREFIX}/share/skills" ]; then
  cp -a "${PREFIX}/share/skills/." "${PREFIX}/skills/"
fi

ln -sfn "${PREFIX}/bin/aos-session" "${BIN_LINK}"

echo "OK. Lancez : agentos-preview"
echo "Données utilisateur conservées sous ${PREFIX}/var"
echo "Désinstall : rm -rf ${PREFIX} ${BIN_LINK}"
