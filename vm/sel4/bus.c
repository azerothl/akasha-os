/*
 * aos-bus — PD de routage d'intents (PV.2).
 *
 * Le gate n'appelle plus capkd directement : tout transite ici
 * (équivalent du Semantic IPC Bus, transport = PPC seL4).
 */
#include <microkit.h>
#include "abi.h"

microkit_msginfo protected(microkit_channel ch, microkit_msginfo msginfo)
{
    seL4_Word op;
    (void)ch;
    op = microkit_msginfo_get_label(msginfo);
    switch (op) {
    case AOS_OP_LOOKUP:
        /* MR0 = opcode demandé ; on sert cap.* (ping/mint/check/revoke). */
        {
            seL4_Word want = microkit_mr_get(0);
            seL4_Word ok = (want == AOS_OP_PING || want == AOS_OP_MINT
                            || want == AOS_OP_CHECK || want == AOS_OP_REVOKE)
                ? AOS_OK
                : AOS_BAD_OP;
            microkit_mr_set(0, ok);
            microkit_mr_set(1, 0);
            return microkit_msginfo_new(0, 2);
        }
    case AOS_OP_PING:
    case AOS_OP_MINT:
    case AOS_OP_CHECK:
    case AOS_OP_REVOKE:
        /* Proxy transparent : les MR du caller sont renvoyés à capkd. */
        return microkit_ppcall(CH_CAPKD, msginfo);
    default:
        microkit_mr_set(0, AOS_BAD_OP);
        microkit_mr_set(1, 0);
        return microkit_msginfo_new(0, 2);
    }
}

void init(void)
{
    microkit_dbg_puts("bus: init\n");
}

void notified(microkit_channel ch)
{
    (void)ch;
}
