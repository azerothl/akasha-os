#!/usr/bin/env bash
# Cloud Agent bootstrap for Akasha OS.
#
# Cloud Agent VMs have no NVIDIA GPU, so this prepares the CPU-only build path
# (packaging/build-preview.sh CPU_ONLY=1). It is idempotent: safe to re-run.
set -euo pipefail

export DEBIAN_FRONTEND=noninteractive

# --- System packages (mirror the Linux CPU CI job in preview-release.yml) ---
sudo apt-get update -qq
sudo apt-get install -y -qq \
  build-essential pkg-config curl ca-certificates git cmake ninja-build \
  clang libclang-dev libssl-dev \
  libx11-dev libxcursor-dev libxrandr-dev libxi-dev libxkbcommon-dev \
  libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
  libwayland-dev libasound2-dev xz-utils zstd findutils lld

# The base image points cc/c++ at clang, whose driver cannot locate libstdc++
# when building llama.cpp (llama-cpp-sys-2) via CMake ("cannot find -lstdc++").
# Force gcc/g++ as the default C/C++ compiler, matching the CI toolchain.
sudo update-alternatives --install /usr/bin/cc  cc  /usr/bin/gcc 100
sudo update-alternatives --install /usr/bin/c++ c++ /usr/bin/g++ 100
sudo update-alternatives --set cc  /usr/bin/gcc
sudo update-alternatives --set c++ /usr/bin/g++

# --- Rust toolchain ---
# Some transitive deps (wasmtime -> cranelift) require the 2024 edition, so a
# recent stable (>= 1.85) is mandatory; the WASM modules target wasm32.
rustup toolchain install stable --profile minimal --no-self-update
rustup default stable
rustup target add wasm32-unknown-unknown

# --- Warm the dependency + build cache (CPU-only, no CUDA) ---
export CPU_ONLY=1 SKIP_MODELS=1 REQUIRE_CUDA=0 GGML_NATIVE=OFF
export CMAKE_GENERATOR=Ninja CMAKE_BUILD_PARALLEL_LEVEL="$(nproc)"
LIBCLANG_DIR="$(dirname "$(ls -1 /usr/lib/llvm-*/lib/libclang.so 2>/dev/null | head -n1)")"
[ -n "${LIBCLANG_DIR}" ] && export LIBCLANG_PATH="${LIBCLANG_DIR}"

cargo fetch --locked || cargo fetch
VERSION="$(tr -d '[:space:]' < VERSION)" bash packaging/build-preview.sh

echo "install.sh: Akasha OS CPU-only build ready."
