#!/usr/bin/env bash
# install-linux.sh — installe / met à jour Akasha OS Preview (non destructif)
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
PREFIX="${PREFIX:-${HOME}/.local/share/agentos-preview}"
BIN_LINK="${HOME}/.local/bin/agentos-preview"

echo "Installation / mise à jour vers ${PREFIX}"
mkdir -p "${PREFIX}" "${HOME}/.local/bin" \
  "${HOME}/.local/share/applications" \
  "${PREFIX}/var" "${PREFIX}/etc" \
  "${PREFIX}/var/mcp" "${PREFIX}/var/skills" "${PREFIX}/var/agents" \
  "${PREFIX}/skills"

# Overlay programme uniquement — jamais --delete sur var/etc.
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

# Seed MCP stub without overwriting user config
if [ ! -f "${PREFIX}/var/mcp/servers.yaml" ]; then
  if [ -f "${PREFIX}/share/mcp/servers.yaml.example" ]; then
    cp -f "${PREFIX}/share/mcp/servers.yaml.example" "${PREFIX}/var/mcp/servers.yaml"
  else
    printf '%s\n' '# MCP servers (stdio)' 'servers: {}' > "${PREFIX}/var/mcp/servers.yaml"
  fi
fi

# Sync packaged skills into working tree
if [ -d "${PREFIX}/share/skills" ]; then
  cp -a "${PREFIX}/share/skills/." "${PREFIX}/skills/"
fi

ln -sfn "${PREFIX}/bin/aos-session" "${BIN_LINK}"

cat > "${HOME}/.local/share/applications/agentos-preview.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Akasha OS Preview
Comment=Akasha OS Preview (NVIDIA)
Exec=env AOS_HOME=${PREFIX} ${PREFIX}/bin/aos-session
Icon=${PREFIX}/share/icons/akasha-os.png
Terminal=false
Categories=Utility;Development;
EOF

echo "OK. Lancez : agentos-preview"
echo "Données utilisateur conservées sous ${PREFIX}/var"
echo "Désinstall : rm -rf ${PREFIX} ${BIN_LINK} ~/.local/share/applications/agentos-preview.desktop"
