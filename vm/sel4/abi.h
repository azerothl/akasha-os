/* Contrat IPC seL4 (piste VM, ADR 0001).
 *
 * Remplace le bus TCP `cap://kernel/<id>` : mêmes opérations mint / check /
 * revoke, transport = protected procedure Microkit (pas de TCP).
 *
 * Mots (MR) : seL4_Word = 64 bits sur qemu_virt_aarch64.
 *   label = opcode
 *   MR0.. = payload
 */
#pragma once

enum {
    AOS_OP_PING = 1,
    AOS_OP_MINT = 2,
    AOS_OP_CHECK = 3,
    AOS_OP_REVOKE = 4,
    AOS_OP_LOOKUP = 5,
    AOS_OP_HW_SMOKE = 6,
};

enum {
    AOS_OK = 0,
    AOS_DENIED = 1,
    AOS_BAD_OP = 2,
    AOS_FULL = 3,
};

/* Droits alignés sur `aos_caps::Rights`. */
enum {
    AOS_READ = 1,
    AOS_WRITE = 2,
    AOS_EXECUTE = 4,
    AOS_GRANT = 8,
    AOS_REVOKE = 16,
};

#define AOS_OBJ_WORDS 4
#define AOS_OBJ_BYTES (AOS_OBJ_WORDS * 8)
#define AOS_MAX_CAPS 32

/* Côté gate : ch 0 = bus (PPC), ch 1 = auditd (notify), ch 2 = dev (PPC).
 * Côté bus  : ch 0 = capkd (PPC).
 * Côté dev  : ch 0 = gate (PPC). */
#define CH_BUS 0
#define CH_CAPKD 0
#define CH_AUDIT 1
#define CH_DEV 2
