/*
 * Gate P4 rejoué dans l'invité seL4 (CPU-only, ADR 0001).
 *
 * 1. lookup + mint/check/revoke via le PD bus (pas d'appel direct capkd)
 * 2. revoke → check refusé immédiatement
 * 3. auditd notifié puis arrêté (pd_stop) ; le bus/capkd répondent encore
 */
#include <stdint.h>
#include <microkit.h>
#include "abi.h"

/* Patché par le tool Microkit (id de l'enfant auditd). */
__attribute__((used)) seL4_Word auditd_id;

static void obj_to_mrs(seL4_Uint8 start, const char *s)
{
    unsigned i;
    unsigned off = 0;
    for (i = 0; i < AOS_OBJ_WORDS; i++) {
        seL4_Word w = 0;
        unsigned b;
        for (b = 0; b < 8; b++) {
            unsigned char c = (unsigned char)s[off];
            w |= ((seL4_Word)c) << (b * 8);
            if (c != 0) {
                off++;
            }
        }
        microkit_mr_set((seL4_Uint8)(start + i), w);
    }
}

static int ping_bus(void)
{
    (void)microkit_ppcall(CH_BUS, microkit_msginfo_new(AOS_OP_PING, 0));
    return microkit_mr_get(0) == AOS_OK;
}

static int lookup(seL4_Word op)
{
    microkit_mr_set(0, op);
    (void)microkit_ppcall(CH_BUS, microkit_msginfo_new(AOS_OP_LOOKUP, 1));
    return microkit_mr_get(0) == AOS_OK;
}

static uint64_t mint(const char *object, uint32_t rights)
{
    microkit_mr_set(0, 1);
    microkit_mr_set(1, rights);
    obj_to_mrs(2, object);
    (void)microkit_ppcall(CH_BUS, microkit_msginfo_new(AOS_OP_MINT, 2 + AOS_OBJ_WORDS));
    if (microkit_mr_get(0) != AOS_OK) {
        return 0;
    }
    return microkit_mr_get(1);
}

static int check(uint64_t cap, const char *object, uint32_t rights)
{
    microkit_mr_set(0, 1);
    microkit_mr_set(1, cap);
    microkit_mr_set(2, rights);
    obj_to_mrs(3, object);
    (void)microkit_ppcall(CH_BUS, microkit_msginfo_new(AOS_OP_CHECK, 3 + AOS_OBJ_WORDS));
    return microkit_mr_get(0) == AOS_OK;
}

static int revoke(uint64_t cap)
{
    microkit_mr_set(0, 1);
    microkit_mr_set(1, cap);
    (void)microkit_ppcall(CH_BUS, microkit_msginfo_new(AOS_OP_REVOKE, 2));
    return microkit_mr_get(0) == AOS_OK;
}

static int run_hw_smoke(void)
{
    (void)microkit_ppcall(CH_DEV, microkit_msginfo_new(AOS_OP_HW_SMOKE, 0));
    return microkit_mr_get(0) == AOS_OK;
}

void init(void)
{
    uint64_t cap;
    int failed = 0;
    const char *obj = "fs:/p4/gate.md";

    microkit_dbg_puts("gate: init\n");

    if (!ping_bus()) {
        microkit_dbg_puts("gate: FAIL ping bus\n");
        failed = 1;
    }
    if (!lookup(AOS_OP_MINT) || !lookup(AOS_OP_CHECK) || !lookup(AOS_OP_REVOKE)) {
        microkit_dbg_puts("gate: FAIL lookup cap.*\n");
        failed = 1;
    } else {
        microkit_dbg_puts("gate: bus lookup cap.* OK\n");
    }

    cap = mint(obj, (uint32_t)(AOS_READ | AOS_WRITE));
    if (cap == 0) {
        microkit_dbg_puts("gate: FAIL mint\n");
        failed = 1;
    } else if (!check(cap, obj, (uint32_t)AOS_READ)) {
        microkit_dbg_puts("gate: FAIL check avant revoke\n");
        failed = 1;
    } else if (!revoke(cap)) {
        microkit_dbg_puts("gate: FAIL revoke\n");
        failed = 1;
    } else if (check(cap, obj, (uint32_t)AOS_READ)) {
        microkit_dbg_puts("gate: FAIL check apres revoke (devrait refuser)\n");
        failed = 1;
    } else {
        microkit_dbg_puts("gate: revoke immediate OK\n");
    }

    microkit_notify(CH_AUDIT);
    microkit_pd_stop((microkit_child)auditd_id);
    microkit_dbg_puts("gate: auditd stopped\n");

    if (!ping_bus()) {
        microkit_dbg_puts("gate: FAIL bus/capkd mort apres stop auditd\n");
        failed = 1;
    } else {
        microkit_dbg_puts("gate: bus+capkd alive after auditd stop\n");
    }

    if (failed) {
        microkit_dbg_puts("AOS_GATE_VM_FAIL\n");
    } else {
        microkit_dbg_puts("AOS_GATE_VM_PASS\n");
    }

    if (!run_hw_smoke()) {
        microkit_dbg_puts("gate: FAIL hw smoke (fb/kbd/blk/net)\n");
    } else {
        microkit_dbg_puts("gate: hw smoke OK\n");
    }
}

void notified(microkit_channel ch)
{
    (void)ch;
}

seL4_Bool fault(microkit_child child, microkit_msginfo msginfo, microkit_msginfo *reply_msginfo)
{
    (void)child;
    (void)msginfo;
    (void)reply_msginfo;
    microkit_dbg_puts("gate: child fault (ignored)\n");
    return seL4_False;
}
