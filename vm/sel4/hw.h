/* QEMU virt (aarch64) device map for the internal PV gate.
 *
 * Device order must match run.sh -device arguments:
 *   0: virtio-net   @ 0x0a000000  IRQ 16
 *   1: virtio-blk   @ 0x0a000200  IRQ 17
 *   2: virtio-input @ 0x0a000400  IRQ 18
 */
#pragma once

#include <stdint.h>

#define HW_VIRTIO_MMIO_PADDR  0x0a003000u
#define HW_VIRTIO_NET_OFF     0x0000u
#define HW_VIRTIO_BLK_OFF     0x0200u
#define HW_VIRTIO_IN_OFF      0x0400u

#define HW_VIRTIO_NET_PADDR  (HW_VIRTIO_MMIO_PADDR + HW_VIRTIO_NET_OFF)
#define HW_VIRTIO_BLK_PADDR  (HW_VIRTIO_MMIO_PADDR + HW_VIRTIO_BLK_OFF)
#define HW_VIRTIO_IN_PADDR   (HW_VIRTIO_MMIO_PADDR + HW_VIRTIO_IN_OFF)

#define HW_IRQ_NET 16u
#define HW_IRQ_BLK 17u
#define HW_IRQ_IN  18u

/* Preview cut-face void (#070b14), 32-bit XRGB for the smoke surface. */
#define HW_FB_VOID_ARGB 0xff070b14u

#define HW_FB_WIDTH  640u
#define HW_FB_HEIGHT 480u
#define HW_FB_BPP    4u
#define HW_FB_BYTES  ((uint32_t)HW_FB_WIDTH * HW_FB_HEIGHT * HW_FB_BPP)

#define HW_BLK_SECTOR_BYTES 512u
#define HW_DISK_MAGIC       "AOS_GATEDISK"
