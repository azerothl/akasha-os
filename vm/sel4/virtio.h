/* Minimal virtio-mmio + single-queue helpers (PV gate smoke only). */
#pragma once

#include <stdint.h>
#include <stdbool.h>

enum {
    VIRTIO_ID_NET   = 1,
    VIRTIO_ID_BLOCK = 2,
    VIRTIO_ID_INPUT = 18,
};

enum {
    VIRTIO_MMIO_MAGIC = 0x74726976u,
};

enum {
    VIRTIO_STATUS_ACK       = 1,
    VIRTIO_STATUS_DRIVER    = 2,
    VIRTIO_STATUS_DRIVER_OK = 4,
    VIRTIO_STATUS_FEATURES_OK = 8,
    VIRTIO_STATUS_FAILED    = 128,
};

enum {
    VIRTIO_BLK_T_IN  = 0,
    VIRTIO_BLK_T_OUT = 1,
};

enum {
    VIRTIO_BLK_S_OK     = 0,
    VIRTIO_BLK_S_IOERR  = 1,
    VIRTIO_BLK_S_UNSUPP = 2,
};

enum {
    VIRTQ_DESC_F_NEXT  = 1,
    VIRTQ_DESC_F_WRITE = 2,
};

#define VIRTIO_LEGACY_MMIO 1

#define VIRTQ_SIZE 8u
#define VIRTQ_BYTES 8192u

struct virtio_desc {
    uint64_t addr;
    uint32_t len;
    uint16_t flags;
    uint16_t next;
};

/* Opaque MMIO base; offsets follow linux/virtio_mmio.h */
struct virtio_mmio {
    volatile uint8_t raw[0x100];
};

struct virtqueue {
    uint8_t *mem;
    uint16_t last_used_idx;
};

struct virtio_dev {
    volatile struct virtio_mmio *mmio;
    struct virtqueue *queue;
    uintptr_t queue_paddr;
};

struct virtio_blk_io {
    uint32_t type;
    uint32_t reserved;
    uint64_t sector;
    uint8_t data[512];
    uint8_t status;
};

bool virtio_probe(volatile struct virtio_mmio *mmio, uint32_t expect_id);
bool virtio_init(struct virtio_dev *dev, volatile struct virtio_mmio *mmio,
                 struct virtqueue *queue, uintptr_t queue_paddr);
void virtio_irq_ack(struct virtio_dev *dev);
bool virtio_blk_xfer(struct virtio_dev *dev, struct virtio_blk_io *io,
                     uintptr_t io_paddr, uint64_t sector, unsigned len,
                     int write);
bool virtio_net_mac(struct virtio_dev *dev, uint8_t mac_out[6]);
bool virtio_input_event(struct virtio_dev *dev, uint16_t *type,
                        uint16_t *code, uint32_t *value,
                        unsigned max_spins, void *ev_vaddr,
                        uintptr_t ev_paddr);
