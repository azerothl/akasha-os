#!/usr/bin/env bash
# uninstall-linux-service.sh — remove opt-in aos-serverd systemd --user unit (P21.5)
set -euo pipefail

UNIT_NAME="aos-serverd.service"
UNIT_DIR="${XDG_CONFIG_HOME:-${HOME}/.config}/systemd/user"
UNIT_PATH="${UNIT_DIR}/${UNIT_NAME}"

if command -v systemctl >/dev/null 2>&1; then
  systemctl --user disable --now "${UNIT_NAME}" 2>/dev/null || true
  systemctl --user daemon-reload 2>/dev/null || true
fi

rm -f "${UNIT_PATH}"
echo "OK. Removed ${UNIT_NAME} (if present). Desktop aos-session path unchanged."
