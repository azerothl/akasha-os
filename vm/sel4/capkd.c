/*
 * aos-capkd — protection domain seL4 (piste VM, ADR 0001).
 *
 * Glue Microkit : mint / check / revoke délégués à `aos-caps::CapStore`
 * (staticlib `aos-sel4-capkd`). Point d'application unique : une révocation
 * ici est immédiatement visible par tout appelant (PPC), sans cache local.
 */
#include <stdint.h>
#include <microkit.h>
#include "abi.h"
#include "aos_cap.h"

static void obj_from_mrs(char *dst, seL4_Uint8 start_mr)
{
    unsigned i;
    for (i = 0; i < AOS_OBJ_WORDS; i++) {
        seL4_Word w = microkit_mr_get((seL4_Uint8)(start_mr + i));
        unsigned b;
        for (b = 0; b < 8; b++) {
            dst[i * 8 + b] = (char)((w >> (b * 8)) & 0xff);
        }
    }
    dst[AOS_OBJ_BYTES - 1] = 0;
}

static microkit_msginfo reply(seL4_Word status, seL4_Word cap_id)
{
    microkit_mr_set(0, status);
    microkit_mr_set(1, cap_id);
    return microkit_msginfo_new(0, 2);
}

static microkit_msginfo do_mint(void)
{
    uint64_t holder = microkit_mr_get(0);
    uint32_t rights = (uint32_t)microkit_mr_get(1) | (uint32_t)AOS_REVOKE;
    char object[AOS_OBJ_BYTES];
    uint64_t id;
    obj_from_mrs(object, 2);
    id = aos_cap_mint(holder, object, rights);
    if (id == 0) {
        return reply(AOS_FULL, 0);
    }
    microkit_dbg_puts("capkd: mint ok\n");
    return reply(AOS_OK, id);
}

static microkit_msginfo do_check(void)
{
    uint64_t holder = microkit_mr_get(0);
    uint64_t cap_id = microkit_mr_get(1);
    uint32_t need = (uint32_t)microkit_mr_get(2);
    char object[AOS_OBJ_BYTES];
    obj_from_mrs(object, 3);
    if (aos_cap_check(holder, cap_id, need, object) != 0) {
        return reply(AOS_DENIED, cap_id);
    }
    return reply(AOS_OK, cap_id);
}

static microkit_msginfo do_revoke(void)
{
    uint64_t holder = microkit_mr_get(0);
    uint64_t cap_id = microkit_mr_get(1);
    if (aos_cap_revoke(holder, cap_id) != 0) {
        return reply(AOS_DENIED, cap_id);
    }
    microkit_dbg_puts("capkd: revoke ok\n");
    return reply(AOS_OK, cap_id);
}

microkit_msginfo protected(microkit_channel ch, microkit_msginfo msginfo)
{
    (void)ch;
    switch (microkit_msginfo_get_label(msginfo)) {
    case AOS_OP_PING:
        return reply(AOS_OK, 0);
    case AOS_OP_MINT:
        return do_mint();
    case AOS_OP_CHECK:
        return do_check();
    case AOS_OP_REVOKE:
        return do_revoke();
    default:
        return reply(AOS_BAD_OP, 0);
    }
}

void init(void)
{
    aos_cap_init();
    microkit_dbg_puts("capkd: init\n");
}

void notified(microkit_channel ch)
{
    (void)ch;
}
