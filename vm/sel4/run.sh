#!/usr/bin/env bash
# Build + boot QEMU virt aarch64. Success when serial contains AOS_GATE_VM_PASS
# and AOS_GATE_VM_HW_PASS (framebuffer + virtio blk/net/input smoke).
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
            --release --target aarch64-unknown-none --crate-type staticlib \
            -- -C panic=abort
    else
        echo "libaos_sel4_capkd.a absent et cargo introuvable." >&2
        echo "Sur l'hôte : cargo rustc -p aos-sel4-capkd --release --target aarch64-unknown-none --crate-type staticlib" >&2
        exit 1
    fi
fi

# Windows cargo + WSL make: NTFS mtime can sit a few hundred ms in the future.
touch "${RUST_LIB}"

make -C "${ROOT}" BUILD_DIR="${BUILD_DIR}" MICROKIT_SDK="${MICROKIT_SDK}" \
    MICROKIT_BOARD="${MICROKIT_BOARD}" MICROKIT_CONFIG="${MICROKIT_CONFIG}" \
    LLVM="${LLVM}"

DISK="${BUILD_DIR}/gate.disk"
dd if=/dev/zero of="${DISK}" bs=512 count=128 status=none 2>/dev/null
printf 'AOS_GATEDISK' | dd of="${DISK}" bs=1 conv=notrunc status=none 2>/dev/null

IMG="${BUILD_DIR}/loader.img"
LOG="${BUILD_DIR}/qemu.log"
MON="${BUILD_DIR}/qemu-mon.sock"
rm -f "${MON}" "${LOG}"

echo "== QEMU ${MICROKIT_BOARD} (fb + virtio blk/net/input) =="
set +e
timeout 25 qemu-system-aarch64 \
    -machine virt,virtualization=on \
    -cpu cortex-a53 \
    -global virtio-mmio.force-legacy=on \
    -nographic \
    -serial mon:stdio \
    -monitor "unix:${MON},server=on,wait=off" \
    -device loader,file="${IMG}",addr=0x70000000,cpu-num=0 \
    -m size=2G \
    -netdev user,id=net0 \
    -device virtio-net-device,netdev=net0,mac=52:55:00:d1:55:01,bus=virtio-mmio-bus.24 \
    -drive if=none,id=hd0,file="${DISK}",format=raw \
    -device virtio-blk-device,drive=hd0,bus=virtio-mmio-bus.25 \
    -device virtio-keyboard-device,bus=virtio-mmio-bus.26 \
    > "${LOG}" 2>&1 &
QEMU_PID=$!

(
    for _ in $(seq 1 120); do
        if [ -S "${MON}" ]; then
            printf 'sendkey a\n' | nc -U "${MON}" >/dev/null 2>&1 || true
        fi
        sleep 0.15
    done
) &
KEY_PID=$!

for _ in $(seq 1 80); do
    if [ -S "${MON}" ]; then
        break
    fi
    sleep 0.1
done

wait "${QEMU_PID}"
QEMU_RC=$?
kill "${KEY_PID}" 2>/dev/null || true
wait "${KEY_PID}" 2>/dev/null || true
set -e

cat "${LOG}"

PASS_CAP=0
PASS_HW=0
grep -q "AOS_GATE_VM_PASS" "${LOG}" && PASS_CAP=1
grep -q "AOS_GATE_VM_HW_PASS" "${LOG}" && PASS_HW=1

if [ "${PASS_CAP}" -eq 1 ] && [ "${PASS_HW}" -eq 1 ]; then
    echo
    echo "=== Gate VM seL4 : PASS (caps + hw) ==="
    exit 0
fi

echo
if [ "${PASS_CAP}" -ne 1 ]; then
    echo "=== Gate VM seL4 : FAIL (pas de AOS_GATE_VM_PASS) ===" >&2
fi
if [ "${PASS_HW}" -ne 1 ]; then
    echo "=== Gate VM seL4 : FAIL (pas de AOS_GATE_VM_HW_PASS) ===" >&2
fi
exit 1
