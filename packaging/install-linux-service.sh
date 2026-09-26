#!/usr/bin/env bash
# install-linux-service.sh — opt-in systemd --user unit for aos-serverd (P21.5)
#
# Default Preview install does NOT run this. Desktop stays shortcut → aos-session.
# Requires: systemd user session, aos-serverd already installed under PREFIX.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
PREFIX="${PREFIX:-${AOS_HOME:-${HOME}/.local/share/agentos-preview}}"
UNIT_NAME="aos-serverd.service"
UNIT_DIR="${XDG_CONFIG_HOME:-${HOME}/.config}/systemd/user"
TEMPLATE="${HERE}/services/aos-serverd.service"
# Prefer binary next to this script (release extract), else PREFIX/bin.
if [ -x "${HERE}/bin/aos-serverd" ]; then
  BIN="${HERE}/bin/aos-serverd"
elif [ -x "${PREFIX}/bin/aos-serverd" ]; then
  BIN="${PREFIX}/bin/aos-serverd"
else
  echo "ERROR: aos-serverd not found under ${HERE}/bin or ${PREFIX}/bin" >&2
  echo "Install Preview first (./install.sh), then re-run this script." >&2
  exit 1
fi

if ! command -v systemctl >/dev/null 2>&1; then
  echo "ERROR: systemctl not found — systemd user services unavailable." >&2
  exit 1
fi

if [ ! -f "${TEMPLATE}" ]; then
  echo "ERROR: missing unit template ${TEMPLATE}" >&2
  exit 1
fi

mkdir -p "${UNIT_DIR}" "${PREFIX}/var/run"
UNIT_PATH="${UNIT_DIR}/${UNIT_NAME}"

sed -e "s|@AOS_HOME@|${PREFIX}|g" -e "s|@AOS_BIN@|${BIN}|g" \
  "${TEMPLATE}" > "${UNIT_PATH}"

systemctl --user daemon-reload
systemctl --user enable --now "${UNIT_NAME}"

echo "OK. systemd --user unit enabled: ${UNIT_NAME}"
echo "  AOS_HOME=${PREFIX}"
echo "  ExecStart=${BIN} serve"
echo "Status : systemctl --user status ${UNIT_NAME}"
echo "Control: ${BIN} --aos-home ${PREFIX} status"
echo "Remove : ${HERE}/uninstall-linux-service.sh"
