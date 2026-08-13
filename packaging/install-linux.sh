#!/usr/bin/env bash
# install-linux.sh — installe Agent OS Preview pour l'utilisateur courant
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
PREFIX="${PREFIX:-${HOME}/.local/share/agentos-preview}"
BIN_LINK="${HOME}/.local/bin/agentos-preview"

echo "Installation vers ${PREFIX}"
mkdir -p "${PREFIX}" "${HOME}/.local/bin" \
  "${HOME}/.local/share/applications"

rsync -a --delete "${HERE}/" "${PREFIX}/" 2>/dev/null \
  || (rm -rf "${PREFIX}" && cp -a "${HERE}" "${PREFIX}")

ln -sfn "${PREFIX}/bin/aos-session" "${BIN_LINK}"

cat > "${HOME}/.local/share/applications/agentos-preview.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Agent OS Preview
Comment=Agent OS Preview 0.1 (NVIDIA)
Exec=env AOS_HOME=${PREFIX} ${PREFIX}/bin/aos-session
Icon=utilities-terminal
Terminal=false
Categories=Utility;Development;
EOF

echo "OK. Lancez : agentos-preview"
echo "Désinstall : rm -rf ${PREFIX} ${BIN_LINK} ~/.local/share/applications/agentos-preview.desktop"
