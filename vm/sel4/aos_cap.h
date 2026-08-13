/* ABI C du CapStore Rust (`aos-sel4-capkd`), lié dans le PD capkd. */
#pragma once

#include <stdint.h>

void aos_cap_init(void);
uint64_t aos_cap_mint(uint64_t holder, const char *object, uint32_t rights);
int aos_cap_check(uint64_t holder, uint64_t cap, uint32_t rights, const char *object);
int aos_cap_revoke(uint64_t holder, uint64_t cap);
