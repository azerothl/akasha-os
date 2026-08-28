/*
 * dev — PV hardware smoke PD (framebuffer surface + virtio blk/net/input).
 *
 * Invoked by the gate after the P4 cap replay. Reports per-device status via
 * serial markers consumed by run.sh / CI.
 */
#include <stdint.h>
#include <stddef.h>
#include <microkit.h>
#include "abi.h"
#include "hw.h"
#include "virtio.h"

uintptr_t fb_surface_vaddr;
uintptr_t fb_surface_paddr;

uintptr_t virtio_mmio_vaddr;

uintptr_t virtio_dma_vaddr;
uintptr_t virtio_dma_paddr;

uintptr_t virtio_net_queue_vaddr;
uintptr_t virtio_blk_queue_vaddr;
uintptr_t virtio_in_queue_vaddr;

uintptr_t virtio_net_queue_paddr;
uintptr_t virtio_blk_queue_paddr;
uintptr_t virtio_in_queue_paddr;

static struct virtqueue net_queue;
static struct virtqueue blk_queue;
static struct virtqueue in_queue;

static struct virtio_dev net_dev;
static struct virtio_dev blk_dev;
static struct virtio_dev in_dev;

static int
cmp_mem(const void *a, const void *b, unsigned n)
{
    const uint8_t *x = a;
    const uint8_t *y = b;
    unsigned i;
    for (i = 0; i < n; i++) {
        if (x[i] != y[i]) {
            return (int)x[i] - (int)y[i];
        }
    }
    return 0;
}

static int
smoke_fb(void)
{
    uint32_t *pixels = (uint32_t *)(uintptr_t)fb_surface_vaddr;
    uint32_t count = HW_FB_BYTES / HW_FB_BPP;
    uint32_t i;

    if (fb_surface_vaddr == 0) {
        microkit_dbg_puts("dev: fb FAIL (no mapping)\n");
        return 0;
    }

    for (i = 0; i < count; i++) {
        pixels[i] = HW_FB_VOID_ARGB;
    }
    if (pixels[0] != HW_FB_VOID_ARGB || pixels[count - 1] != HW_FB_VOID_ARGB) {
        microkit_dbg_puts("dev: fb FAIL (verify)\n");
        return 0;
    }

    microkit_dbg_puts("dev: fb surface painted OK\n");
    microkit_dbg_puts("AOS_GATE_VM_FB\n");
    return 1;
}

static int
smoke_blk(void)
{
    volatile struct virtio_mmio *blk_mmio =
        (volatile struct virtio_mmio *)(virtio_mmio_vaddr + HW_VIRTIO_BLK_OFF);
    struct virtio_blk_io *io = (struct virtio_blk_io *)(uintptr_t)virtio_dma_vaddr;
    uintptr_t io_paddr = virtio_dma_paddr;
    const char stamp[] = "AOS_GATE_BLK_OK!";

    if (!virtio_probe(blk_mmio, VIRTIO_ID_BLOCK)) {
        microkit_dbg_puts("dev: blk FAIL (probe)\n");
        return 0;
    }
    if (!virtio_init(&blk_dev,
                     (volatile struct virtio_mmio *)(virtio_mmio_vaddr + HW_VIRTIO_BLK_OFF),
                     &blk_queue, virtio_blk_queue_paddr)) {
        microkit_dbg_puts("dev: blk FAIL (init)\n");
        return 0;
    }

    if (!virtio_blk_xfer(&blk_dev, io, io_paddr, 0, sizeof(io->data), 0)) {
        microkit_dbg_puts("dev: blk FAIL (read)\n");
        return 0;
    }
    if (cmp_mem(io->data, HW_DISK_MAGIC, sizeof(HW_DISK_MAGIC) - 1) != 0) {
        microkit_dbg_puts("dev: blk FAIL (magic)\n");
        return 0;
    }

    for (unsigned i = 0; i < sizeof(io->data); i++) {
        io->data[i] = (uint8_t)(i ^ 0xa5u);
    }
    for (unsigned i = 0; i < sizeof(stamp) - 1; i++) {
        io->data[i] = (uint8_t)stamp[i];
    }
    if (!virtio_blk_xfer(&blk_dev, io, io_paddr, 1, sizeof(io->data), 1)) {
        microkit_dbg_puts("dev: blk FAIL (write)\n");
        return 0;
    }
    for (unsigned i = 0; i < sizeof(io->data); i++) {
        io->data[i] = 0;
    }
    if (!virtio_blk_xfer(&blk_dev, io, io_paddr, 1, sizeof(io->data), 0)) {
        microkit_dbg_puts("dev: blk FAIL (readback)\n");
        return 0;
    }
    if (cmp_mem(io->data, stamp, sizeof(stamp) - 1) != 0) {
        microkit_dbg_puts("dev: blk FAIL (verify)\n");
        return 0;
    }

    microkit_dbg_puts("dev: virtio-blk read/write OK\n");
    microkit_dbg_puts("AOS_GATE_VM_BLK\n");
    return 1;
}

static int
smoke_net(void)
{
    uint8_t mac[6];

    if (!virtio_probe((volatile struct virtio_mmio *)(virtio_mmio_vaddr + HW_VIRTIO_NET_OFF),
                      VIRTIO_ID_NET)) {
        microkit_dbg_puts("dev: net FAIL (probe)\n");
        return 0;
    }
    if (!virtio_init(&net_dev,
                     (volatile struct virtio_mmio *)(virtio_mmio_vaddr + HW_VIRTIO_NET_OFF),
                     &net_queue, virtio_net_queue_paddr)) {
        microkit_dbg_puts("dev: net FAIL (init)\n");
        return 0;
    }
    if (!virtio_net_mac(&net_dev, mac)) {
        microkit_dbg_puts("dev: net FAIL (mac)\n");
        return 0;
    }

    microkit_dbg_puts("dev: virtio-net MAC visible OK\n");
    microkit_dbg_puts("AOS_GATE_VM_NET\n");
    return 1;
}

static int
smoke_kbd(void)
{
    uint16_t type = 0;
    uint16_t code = 0;
    uint32_t value = 0;
    struct {
        uint16_t type;
        uint16_t code;
        uint32_t value;
    } *ev = (void *)(uintptr_t)(virtio_dma_vaddr + 0x400u);
    uintptr_t ev_paddr = virtio_dma_paddr + 0x400u;

    if (!virtio_probe((volatile struct virtio_mmio *)(virtio_mmio_vaddr + HW_VIRTIO_IN_OFF),
                      VIRTIO_ID_INPUT)) {
        microkit_dbg_puts("dev: kbd FAIL (probe)\n");
        return 0;
    }
    if (!virtio_init(&in_dev,
                     (volatile struct virtio_mmio *)(virtio_mmio_vaddr + HW_VIRTIO_IN_OFF),
                     &in_queue, virtio_in_queue_paddr)) {
        microkit_dbg_puts("dev: kbd FAIL (init)\n");
        return 0;
    }

    for (unsigned attempt = 0; attempt < 40u; attempt++) {
        if (virtio_input_event(&in_dev, &type, &code, &value, 10000u, ev, ev_paddr)) {
            microkit_dbg_puts("dev: virtio-input event OK\n");
            microkit_dbg_puts("AOS_GATE_VM_KBD\n");
            return 1;
        }
        virtio_irq_ack(&in_dev);
    }

    microkit_dbg_puts("dev: kbd FAIL (timeout)\n");
    return 0;
}

static microkit_msginfo
run_hw_smoke(void)
{
    int ok = smoke_fb() && smoke_blk() && smoke_net() && smoke_kbd();
    if (ok) {
        microkit_dbg_puts("AOS_GATE_VM_HW_PASS\n");
        microkit_mr_set(0, AOS_OK);
    } else {
        microkit_dbg_puts("AOS_GATE_VM_HW_FAIL\n");
        microkit_mr_set(0, AOS_DENIED);
    }
    return microkit_msginfo_new(0, 1);
}

microkit_msginfo
protected(microkit_channel ch, microkit_msginfo msginfo)
{
    (void)ch;
    switch (microkit_msginfo_get_label(msginfo)) {
    case AOS_OP_HW_SMOKE:
        return run_hw_smoke();
    default:
        microkit_mr_set(0, AOS_BAD_OP);
        return microkit_msginfo_new(0, 1);
    }
}

void
init(void)
{
    net_queue.mem = (uint8_t *)(uintptr_t)virtio_net_queue_vaddr;
    net_queue.last_used_idx = 0;
    blk_queue.mem = (uint8_t *)(uintptr_t)virtio_blk_queue_vaddr;
    blk_queue.last_used_idx = 0;
    in_queue.mem = (uint8_t *)(uintptr_t)virtio_in_queue_vaddr;
    in_queue.last_used_idx = 0;
    microkit_dbg_puts("dev: init\n");
}

void
notified(microkit_channel ch)
{
    (void)ch;
}
