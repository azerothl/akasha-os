#!/usr/bin/env bash
# uninstall-macos-service.sh — remove opt-in aos-serverd LaunchAgent (P21.5)
set -euo pipefail

LABEL="com.azerothl.aos-serverd"
PLIST_PATH="${HOME}/Library/LaunchAgents/${LABEL}.plist"

if [ "$(uname -s)" = "Darwin" ]; then
  launchctl bootout "gui/$(id -u)/${LABEL}" 2>/dev/null || true
fi

rm -f "${PLIST_PATH}"
echo "OK. Removed LaunchAgent ${LABEL} (if present). Desktop aos-session path unchanged."
