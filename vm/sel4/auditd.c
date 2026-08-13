/*
 * aos-auditd — PD enfant du gate. Isolé : l'arrêter (pd_stop) ne doit
 * pas empêcher capkd de répondre (rejeu du critère P4 « kill Audit »).
 */
#include <microkit.h>

void init(void)
{
    microkit_dbg_puts("auditd: init\n");
}

void notified(microkit_channel ch)
{
    (void)ch;
    microkit_dbg_puts("auditd: append (alive)\n");
}
