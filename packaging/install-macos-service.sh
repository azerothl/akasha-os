#!/usr/bin/env bash
# install-macos-service.sh — opt-in LaunchAgent for aos-serverd (P21.5)
#
# Default Preview install does NOT run this. Desktop stays agentos-preview → aos-session.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
PREFIX="${PREFIX:-${AOS_HOME:-${HOME}/.local/share/agentos-preview}}"
LABEL="com.azerothl.aos-serverd"
AGENTS_DIR="${HOME}/Library/LaunchAgents"
PLIST_PATH="${AGENTS_DIR}/${LABEL}.plist"
TEMPLATE="${HERE}/services/com.azerothl.aos-serverd.plist"

if [ "$(uname -s)" != "Darwin" ]; then
  echo "ERROR: macOS only." >&2
  exit 1
fi

if [ -x "${HERE}/bin/aos-serverd" ]; then
  BIN="${HERE}/bin/aos-serverd"
elif [ -x "${PREFIX}/bin/aos-serverd" ]; then
  BIN="${PREFIX}/bin/aos-serverd"
else
  echo "ERROR: aos-serverd not found under ${HERE}/bin or ${PREFIX}/bin" >&2
  echo "Install Preview first (./install.sh), then re-run this script." >&2
  exit 1
fi

if [ ! -f "${TEMPLATE}" ]; then
  echo "ERROR: missing LaunchAgent template ${TEMPLATE}" >&2
  exit 1
fi

mkdir -p "${AGENTS_DIR}" "${PREFIX}/var/run"

# Clear quarantine on binary if present (unsigned Preview builds).
if command -v xattr >/dev/null 2>&1; then
  xattr -cr "${BIN}" 2>/dev/null || true
fi

sed -e "s|@AOS_HOME@|${PREFIX}|g" \
    -e "s|@AOS_BIN@|${BIN}|g" \
    -e "s|@AOS_LABEL@|${LABEL}|g" \
  "${TEMPLATE}" > "${PLIST_PATH}"

# Reload if already loaded.
launchctl bootout "gui/$(id -u)/${LABEL}" 2>/dev/null || true
launchctl bootstrap "gui/$(id -u)" "${PLIST_PATH}"
launchctl enable "gui/$(id -u)/${LABEL}" 2>/dev/null || true
launchctl kickstart -k "gui/$(id -u)/${LABEL}" 2>/dev/null || true

echo "OK. LaunchAgent loaded: ${LABEL}"
echo "  AOS_HOME=${PREFIX}"
echo "  Program=${BIN} serve"
echo "Status : launchctl print gui/$(id -u)/${LABEL}"
echo "Control: ${BIN} --aos-home ${PREFIX} status"
echo "Remove : ${HERE}/uninstall-macos-service.sh"
