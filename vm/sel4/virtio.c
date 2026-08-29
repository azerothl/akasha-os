/*
 * Minimal virtio-mmio helpers for the internal PV gate (sync poll only).
 */
#include <stdint.h>
#include <stddef.h>
#include "virtio.h"

enum {
    MMIO_MAGIC            = 0x000,
    MMIO_VERSION          = 0x004,
    MMIO_DEVICE_ID        = 0x008,
    MMIO_DEVICE_FEATURES  = 0x010,
    MMIO_DEVICE_FEAT_SEL  = 0x014,
    MMIO_DRIVER_FEATURES  = 0x020,
    MMIO_DRIVER_FEAT_SEL  = 0x024,
    MMIO_GUEST_PAGE_SIZE  = 0x028,
    MMIO_QUEUE_SEL        = 0x030,
    MMIO_QUEUE_NUM_MAX    = 0x034,
    MMIO_QUEUE_NUM        = 0x038,
    MMIO_QUEUE_ALIGN      = 0x03c,
    MMIO_QUEUE_PFN        = 0x040,
    MMIO_QUEUE_NOTIFY     = 0x050,
    MMIO_INTERRUPT_STATUS = 0x060,
    MMIO_INTERRUPT_ACK    = 0x064,
    MMIO_STATUS           = 0x070,
    MMIO_QUEUE_DESC_LOW   = 0x080,
    MMIO_QUEUE_DESC_HIGH  = 0x084,
    MMIO_QUEUE_AVAIL_LOW  = 0x090,
    MMIO_QUEUE_AVAIL_HIGH = 0x094,
    MMIO_QUEUE_USED_LOW   = 0x0a0,
    MMIO_QUEUE_USED_HIGH  = 0x0a4,
    MMIO_CONFIG           = 0x100,
};

static inline volatile uint32_t *
mmio_reg(volatile struct virtio_mmio *mmio, unsigned off)
{
    return (volatile uint32_t *)(mmio->raw + off);
}

static inline struct virtio_desc *
vq_desc(struct virtqueue *q)
{
    return (struct virtio_desc *)q->mem;
}

static inline volatile uint16_t *
vq_avail_idx(struct virtqueue *q)
{
    return (volatile uint16_t *)(q->mem + (VIRTQ_SIZE * sizeof(struct virtio_desc)) + 2);
}

static inline volatile uint16_t *
vq_avail_ring(struct virtqueue *q)
{
    return (volatile uint16_t *)(q->mem + (VIRTQ_SIZE * sizeof(struct virtio_desc)) + 4);
}

static inline volatile uint16_t *
vq_used_idx(struct virtqueue *q)
{
    return (volatile uint16_t *)(q->mem + 4096u + 2);
}

static inline volatile uint32_t *
vq_used_elem_id(struct virtqueue *q, unsigned slot)
{
    return (volatile uint32_t *)(q->mem + 4096u + 4 + (slot * 8u));
}

static inline void
mmio_write32(volatile struct virtio_mmio *mmio, unsigned off, uint32_t val)
{
    *mmio_reg(mmio, off) = val;
}

static inline uint32_t
mmio_read32(volatile struct virtio_mmio *mmio, unsigned off)
{
    return *mmio_reg(mmio, off);
}

static inline void
virtio_mb(void)
{
    __asm__ volatile("dmb sy" ::: "memory");
}

static void
set_status(struct virtio_dev *dev, uint32_t bits)
{
    mmio_write32(dev->mmio, MMIO_STATUS, bits);
}

static void
or_status(struct virtio_dev *dev, uint32_t bits)
{
    set_status(dev, mmio_read32(dev->mmio, MMIO_STATUS) | bits);
}

static void
queue_sync_legacy(struct virtio_dev *dev)
{
    uintptr_t pfn = dev->queue_paddr >> 12;

    mmio_write32(dev->mmio, MMIO_QUEUE_SEL, 0);
    mmio_write32(dev->mmio, MMIO_QUEUE_PFN, 0);
    mmio_write32(dev->mmio, MMIO_QUEUE_NUM, VIRTQ_SIZE);
    mmio_write32(dev->mmio, MMIO_QUEUE_ALIGN, 4096u);
    mmio_write32(dev->mmio, MMIO_GUEST_PAGE_SIZE, 4096u);
    mmio_write32(dev->mmio, MMIO_QUEUE_PFN, (uint32_t)pfn);
}

static void
queue_sync_modern(struct virtio_dev *dev)
{
    uintptr_t base = dev->queue_paddr;
    uint64_t desc = base;
    uint64_t avail = base + (VIRTQ_SIZE * sizeof(struct virtio_desc));
    uint64_t used = base + 4096u;

    mmio_write32(dev->mmio, MMIO_QUEUE_SEL, 0);
    mmio_write32(dev->mmio, MMIO_QUEUE_NUM, VIRTQ_SIZE);
    mmio_write32(dev->mmio, MMIO_QUEUE_DESC_LOW, (uint32_t)desc);
    mmio_write32(dev->mmio, MMIO_QUEUE_DESC_HIGH, (uint32_t)(desc >> 32));
    mmio_write32(dev->mmio, MMIO_QUEUE_AVAIL_LOW, (uint32_t)avail);
    mmio_write32(dev->mmio, MMIO_QUEUE_AVAIL_HIGH, (uint32_t)(avail >> 32));
    mmio_write32(dev->mmio, MMIO_QUEUE_USED_LOW, (uint32_t)used);
    mmio_write32(dev->mmio, MMIO_QUEUE_USED_HIGH, (uint32_t)(used >> 32));
    mmio_write32(dev->mmio, 0x044u, 1);
}

static void
queue_sync(struct virtio_dev *dev)
{
#if VIRTIO_LEGACY_MMIO
    queue_sync_legacy(dev);
#else
    queue_sync_modern(dev);
#endif
}

static bool
queue_submit(struct virtio_dev *dev, unsigned desc_idx, unsigned wait_id,
             unsigned max_spins)
{
    struct virtqueue *q = dev->queue;
    volatile uint16_t *avail_idx = vq_avail_idx(q);
    volatile uint16_t *avail_ring = vq_avail_ring(q);
    volatile uint16_t *used_idx = vq_used_idx(q);
    uint16_t slot = *avail_idx % VIRTQ_SIZE;

    avail_ring[slot] = (uint16_t)desc_idx;
    virtio_mb();
    (*avail_idx)++;
    virtio_mb();
    mmio_write32(dev->mmio, MMIO_QUEUE_NOTIFY, 0);

    for (unsigned spin = 0; spin < max_spins; spin++) {
        virtio_mb();
        if (*used_idx != q->last_used_idx) {
            volatile uint32_t *elem_id = vq_used_elem_id(q, q->last_used_idx % VIRTQ_SIZE);
            uint32_t id = *elem_id;
            q->last_used_idx++;
            return id == wait_id;
        }
        virtio_irq_ack(dev);
    }
    return false;
}

bool
virtio_probe(volatile struct virtio_mmio *mmio, uint32_t expect_id)
{
    if (mmio_read32(mmio, MMIO_MAGIC) != VIRTIO_MMIO_MAGIC) {
        return false;
    }
    if (mmio_read32(mmio, MMIO_VERSION) != 2u && mmio_read32(mmio, MMIO_VERSION) != 1u) {
        return false;
    }
    return mmio_read32(mmio, MMIO_DEVICE_ID) == expect_id;
}

bool
virtio_init(struct virtio_dev *dev, volatile struct virtio_mmio *mmio,
            struct virtqueue *queue, uintptr_t queue_paddr)
{
    unsigned i;

    dev->mmio = mmio;
    dev->queue = queue;
    dev->queue_paddr = queue_paddr;
    if (queue->mem == NULL) {
        return false;
    }
    for (i = 0; i < VIRTQ_BYTES; i++) {
        queue->mem[i] = 0;
    }
    queue->last_used_idx = 0;
    *vq_avail_idx(queue) = 0;
    *vq_used_idx(queue) = 0;

    set_status(dev, 0);
    or_status(dev, VIRTIO_STATUS_ACK);
    or_status(dev, VIRTIO_STATUS_DRIVER);

#if VIRTIO_LEGACY_MMIO
    mmio_write32(dev->mmio, MMIO_DRIVER_FEAT_SEL, 0);
    mmio_write32(dev->mmio, MMIO_DEVICE_FEAT_SEL, 0);
    mmio_write32(dev->mmio, MMIO_DRIVER_FEATURES,
                 mmio_read32(dev->mmio, MMIO_DEVICE_FEATURES));
#else
    mmio_write32(dev->mmio, MMIO_DEVICE_FEAT_SEL, 0);
    uint32_t feat0 = mmio_read32(dev->mmio, MMIO_DEVICE_FEATURES);
    mmio_write32(dev->mmio, MMIO_DRIVER_FEAT_SEL, 0);
    mmio_write32(dev->mmio, MMIO_DRIVER_FEATURES, feat0);
    mmio_write32(dev->mmio, MMIO_DEVICE_FEAT_SEL, 1);
    uint32_t feat1 = mmio_read32(dev->mmio, MMIO_DEVICE_FEATURES);
    mmio_write32(dev->mmio, MMIO_DRIVER_FEAT_SEL, 1);
    mmio_write32(dev->mmio, MMIO_DRIVER_FEATURES, feat1);
    or_status(dev, VIRTIO_STATUS_FEATURES_OK);
    for (i = 0; i < 1000000u; i++) {
        if ((mmio_read32(dev->mmio, MMIO_STATUS) & VIRTIO_STATUS_FEATURES_OK) != 0) {
            break;
        }
    }
    if ((mmio_read32(dev->mmio, MMIO_STATUS) & VIRTIO_STATUS_FEATURES_OK) == 0) {
        set_status(dev, VIRTIO_STATUS_FAILED);
        return false;
    }
#endif

    mmio_write32(dev->mmio, MMIO_QUEUE_SEL, 0);
    if (mmio_read32(dev->mmio, MMIO_QUEUE_NUM_MAX) < VIRTQ_SIZE) {
        set_status(dev, VIRTIO_STATUS_FAILED);
        return false;
    }
    queue_sync(dev);
    or_status(dev, VIRTIO_STATUS_DRIVER_OK);
    return (mmio_read32(dev->mmio, MMIO_STATUS) & VIRTIO_STATUS_DRIVER_OK) != 0;
}

void
virtio_irq_ack(struct virtio_dev *dev)
{
    uint32_t pending = mmio_read32(dev->mmio, MMIO_INTERRUPT_STATUS);
    if (pending != 0) {
        mmio_write32(dev->mmio, MMIO_INTERRUPT_ACK, pending);
    }
}

bool
virtio_blk_xfer(struct virtio_dev *dev, struct virtio_blk_io *io,
                uintptr_t io_paddr, uint64_t sector, unsigned len, int write)
{
    struct virtio_desc *desc = vq_desc(dev->queue);
    uintptr_t hdr_paddr = io_paddr;
    uintptr_t data_paddr = io_paddr + offsetof(struct virtio_blk_io, data);
    uintptr_t status_paddr = io_paddr + offsetof(struct virtio_blk_io, status);

    if (len == 0 || len > 512u) {
        return false;
    }

    io->type = write ? VIRTIO_BLK_T_OUT : VIRTIO_BLK_T_IN;
    io->reserved = 0;
    io->sector = sector;
    io->status = 0xff;

    desc[0].addr = hdr_paddr;
    desc[0].len = (uint32_t)offsetof(struct virtio_blk_io, data);
    desc[0].flags = VIRTQ_DESC_F_NEXT;
    desc[0].next = 1;

    desc[1].addr = data_paddr;
    desc[1].len = len;
    desc[1].flags = (write ? 0 : VIRTQ_DESC_F_WRITE) | VIRTQ_DESC_F_NEXT;
    desc[1].next = 2;

    desc[2].addr = status_paddr;
    desc[2].len = 1;
    desc[2].flags = VIRTQ_DESC_F_WRITE;
    desc[2].next = 0;

    virtio_mb();
    if (!queue_submit(dev, 0, 0, 5000000u)) {
        return false;
    }
    virtio_mb();
    return io->status == VIRTIO_BLK_S_OK;
}

bool
virtio_net_mac(struct virtio_dev *dev, uint8_t mac_out[6])
{
    volatile uint8_t *cfg = dev->mmio->raw + MMIO_CONFIG;
    unsigned i;

    for (i = 0; i < 6; i++) {
        mac_out[i] = cfg[i];
    }
    return mac_out[0] != 0 || mac_out[1] != 0 || mac_out[2] != 0
        || mac_out[3] != 0 || mac_out[4] != 0 || mac_out[5] != 0;
}

bool
virtio_input_event(struct virtio_dev *dev, uint16_t *type,
                   uint16_t *code, uint32_t *value,
                   unsigned max_spins, void *ev_vaddr, uintptr_t ev_paddr)
{
    struct virtio_desc *desc = vq_desc(dev->queue);
    struct {
        uint16_t type;
        uint16_t code;
        uint32_t value;
    } *ev = ev_vaddr;

    desc[0].addr = ev_paddr;
    desc[0].len = sizeof(*ev);
    desc[0].flags = VIRTQ_DESC_F_WRITE;
    desc[0].next = 0;

    if (!queue_submit(dev, 0, 0, max_spins)) {
        return false;
    }
    *type = ev->type;
    *code = ev->code;
    *value = ev->value;
    return true;
}
