#!/usr/bin/env bash
# Build Akasha OS Preview Linux x64 inside a CUDA devel container (compile-only).
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq \
  build-essential pkg-config curl ca-certificates git cmake ninja-build \
  findutils \
  clang libclang-dev \
  libssl-dev \
  libx11-dev libxcursor-dev libxrandr-dev libxi-dev libxkbcommon-dev \
  libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
  libwayland-dev libasound2-dev \
  >/dev/null

# bindgen needs libclang (Ubuntu ships libclang-14.so as a symlink)
LIBCLANG_SO="$(
  ls -1 /usr/lib/llvm-*/lib/libclang.so* /usr/lib/x86_64-linux-gnu/libclang-*.so* 2>/dev/null \
    | grep -v 'libclang-cpp' \
    | sort -V \
    | tail -n1 \
    || true
)"
if [ -z "${LIBCLANG_SO}" ]; then
  echo "ERROR: libclang.so not found (install clang/libclang-dev)" >&2
  ls -la /usr/lib/x86_64-linux-gnu/libclang* 2>/dev/null || true
  exit 1
fi
export LIBCLANG_PATH="$(dirname "${LIBCLANG_SO}")"
echo "LIBCLANG_PATH=${LIBCLANG_PATH} (${LIBCLANG_SO})"
LLVM_BIN="$(ls -d /usr/lib/llvm-*/bin 2>/dev/null | sort -V | tail -n1 || true)"
if [ -n "${LLVM_BIN}" ]; then
  export PATH="${LLVM_BIN}:${PATH}"
fi

if ! command -v rustc >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
fi
# shellcheck disable=SC1091
source "$HOME/.cargo/env"
rustup target add wasm32-unknown-unknown

cd /work
export CARGO_TARGET_DIR=/work/target-linux
export CUDA_HOME="${CUDA_HOME:-/usr/local/cuda}"
export PATH="${CUDA_HOME}/bin:${PATH}"
export SKIP_MODELS=1
./packaging/build-preview.sh
echo "LINUX_BUILD_OK"
