#!/usr/bin/env bash
# Build + boot QEMU virt aarch64. Succès si le serial contient AOS_GATE_VM_PASS.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")" && pwd)"
SDK_VER="2.3.0"
export MICROKIT_SDK="${MICROKIT_SDK:-${ROOT}/sdk/microkit-sdk-${SDK_VER}}"
export MICROKIT_BOARD="${MICROKIT_BOARD:-qemu_virt_aarch64}"
export MICROKIT_CONFIG="${MICROKIT_CONFIG:-debug}"
export BUILD_DIR="${BUILD_DIR:-${ROOT}/build}"
export LLVM="${LLVM:-True}"

if [ ! -x "${MICROKIT_SDK}/bin/microkit" ]; then
    echo "SDK absent — lancer vm/sel4/bootstrap.sh" >&2
    exit 1
fi

mkdir -p "${BUILD_DIR}"

REPO="$(cd "${ROOT}/../.." && pwd)"
RUST_LIB="${REPO}/target/aarch64-unknown-none/release/libaos_sel4_capkd.a"
if [ ! -f "${RUST_LIB}" ]; then
    if command -v cargo >/dev/null 2>&1; then
        echo "== aos-sel4-capkd (aarch64-unknown-none) =="
        rustup target add aarch64-unknown-none
        cargo rustc -p aos-sel4-capkd --manifest-path "${REPO}/Cargo.toml" \
            --release --target aarch64-unknown-none --offline --crate-type staticlib \
            -- -C panic=abort
    else
        echo "libaos_sel4_capkd.a absent et cargo introuvable." >&2
        echo "Sur l'hôte : cargo rustc -p aos-sel4-capkd --release --target aarch64-unknown-none --crate-type staticlib" >&2
        exit 1
    fi
fi

make -C "${ROOT}" BUILD_DIR="${BUILD_DIR}" MICROKIT_SDK="${MICROKIT_SDK}" \
    MICROKIT_BOARD="${MICROKIT_BOARD}" MICROKIT_CONFIG="${MICROKIT_CONFIG}" \
    LLVM="${LLVM}"

IMG="${BUILD_DIR}/loader.img"
LOG="${BUILD_DIR}/qemu.log"
echo "== QEMU ${MICROKIT_BOARD} =="
# QEMU ne s'arrête pas tout seul : timeout, puis on juge le serial.
set +e
timeout 12 qemu-system-aarch64 \
    -machine virt,virtualization=on \
    -cpu cortex-a53 \
    -nographic \
    -serial mon:stdio \
    -device loader,file="${IMG}",addr=0x70000000,cpu-num=0 \
    -m size=2G \
    < /dev/null > "${LOG}" 2>&1
set -e

cat "${LOG}"
if grep -q "AOS_GATE_VM_PASS" "${LOG}"; then
    echo
    echo "=== Gate VM seL4 : PASS ==="
    exit 0
fi
echo
echo "=== Gate VM seL4 : FAIL (pas de AOS_GATE_VM_PASS) ===" >&2
exit 1
