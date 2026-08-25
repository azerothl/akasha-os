#!/bin/sh
# Akasha OS Preview — one-liner installer (Linux x64)
# curl -fsSL https://azerothl.github.io/akasha-os/install.sh | sh
set -eu
REPO="azerothl/akasha-os"
PREFIX="${AOS_HOME:-${HOME}/.local/share/agentos-preview}"
LATEST_URL="https://github.com/${REPO}/releases/latest/download/latest.json"
UA="akasha-os-preview-install"

echo "Akasha OS Preview — fetching latest.json"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
curl -fsSL -A "$UA" "$LATEST_URL" -o "$TMP/latest.json"

NAME="$(python3 - "$TMP/latest.json" <<'PY'
import json, sys
doc = json.load(open(sys.argv[1], encoding="utf-8"))
assets = doc.get("assets") or []
pick = None
for a in assets:
    name = a.get("name") or ""
    if a.get("os") == "linux" and "-cpu" not in name:
        pick = a
        break
if pick is None:
    for a in assets:
        name = a.get("name") or ""
        if "linux-x64.tar.gz" in name and "-cpu" not in name:
            pick = a
            break
if not pick or not pick.get("sha256") or not pick.get("name"):
    sys.stderr.write("latest.json has no Linux artefact with sha256 (fail-closed)\n")
    sys.exit(1)
print(pick["name"])
print(pick["sha256"])
print(doc.get("version") or "")
print(doc.get("tag") or "")
PY
)"
FILE="$(printf '%s\n' "$NAME" | sed -n '1p')"
WANT="$(printf '%s\n' "$NAME" | sed -n '2p' | tr 'A-F' 'a-f')"
VER="$(printf '%s\n' "$NAME" | sed -n '3p')"
TAG="$(printf '%s\n' "$NAME" | sed -n '4p')"
if [ -z "$TAG" ]; then
  TAG="v${VER}"
fi
URL="https://github.com/${REPO}/releases/download/${TAG}/${FILE}"
echo "version ${VER}"
echo "url     ${URL}"
echo "sha256  ${WANT}"

curl -fsSL -A "$UA" "$URL" -o "$TMP/$FILE"
GOT="$(sha256sum "$TMP/$FILE" | awk '{print $1}')"
if [ "$GOT" != "$WANT" ]; then
  echo "sha256 mismatch (got $GOT, expected $WANT) — refuse" >&2
  exit 1
fi

mkdir -p "$TMP/extract"
tar -C "$TMP/extract" -xzf "$TMP/$FILE"
ROOT="$TMP/extract"
if [ ! -f "$ROOT/install.sh" ]; then
  FOUND="$(find "$TMP/extract" -maxdepth 2 -name install.sh | head -n1)"
  if [ -n "$FOUND" ]; then
    ROOT="$(dirname "$FOUND")"
  fi
fi
if [ -z "${ROOT:-}" ] || [ ! -f "$ROOT/install.sh" ]; then
  echo "extracted archive missing install.sh overlay" >&2
  exit 1
fi
chmod +x "$ROOT/install.sh"
PREFIX="$PREFIX" "$ROOT/install.sh"
echo "Installed under ${PREFIX} (var/ and etc/ preserved on overlay)."
