#!/usr/bin/env bash
# Télécharge le SDK Microkit 2.3.0 et installe QEMU + toolchain (WSL Ubuntu).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")" && pwd)"
SDK_VER="2.3.0"
SDK_TGZ="microkit-sdk-${SDK_VER}-linux-x86-64.tar.gz"
SDK_URL="https://github.com/seL4/microkit/releases/download/${SDK_VER}/${SDK_TGZ}"
SDK_DIR="${ROOT}/sdk/microkit-sdk-${SDK_VER}"

if ! command -v qemu-system-aarch64 >/dev/null 2>&1 \
   || ! command -v clang >/dev/null 2>&1 \
   || ! command -v aarch64-linux-gnu-gcc >/dev/null 2>&1; then
    echo "== apt : qemu, clang, lld, gcc aarch64 =="
    sudo apt-get update -qq
    sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -qq \
        qemu-system-arm clang lld make wget ca-certificates gcc-aarch64-linux-gnu
fi

mkdir -p "${ROOT}/sdk"
if [ ! -x "${SDK_DIR}/bin/microkit" ]; then
    echo "== SDK Microkit ${SDK_VER} =="
    wget -q --show-progress -O "${ROOT}/sdk/${SDK_TGZ}" "${SDK_URL}"
    tar -C "${ROOT}/sdk" -xzf "${ROOT}/sdk/${SDK_TGZ}"
fi

echo "MICROKIT_SDK=${SDK_DIR}"
if [ -x "${SDK_DIR}/bin/microkit" ]; then
    echo "microkit OK"
fi
