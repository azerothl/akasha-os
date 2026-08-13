ifeq ($(strip $(BUILD_DIR)),)
$(error BUILD_DIR must be specified)
endif
ifeq ($(strip $(MICROKIT_SDK)),)
$(error MICROKIT_SDK must be specified)
endif
ifeq ($(strip $(MICROKIT_BOARD)),)
$(error MICROKIT_BOARD must be specified)
endif
ifeq ($(strip $(MICROKIT_CONFIG)),)
$(error MICROKIT_CONFIG must be specified)
endif

SEL4_ROOT := $(abspath $(dir $(lastword $(MAKEFILE_LIST))))
REPO_ROOT := $(abspath $(SEL4_ROOT)/../..)
RUST_LIB ?= $(REPO_ROOT)/target/aarch64-unknown-none/release/libaos_sel4_capkd.a

BOARD_DIR := $(MICROKIT_SDK)/board/$(MICROKIT_BOARD)/$(MICROKIT_CONFIG)
ARCH := ${shell grep 'CONFIG_SEL4_ARCH ' $(BOARD_DIR)/include/kernel/gen_config.h | cut -d' ' -f4}

ifeq ($(ARCH),aarch64)
  TARGET_TRIPLE := aarch64-none-elf
  CFLAGS_ARCH := -mstrict-align
else
$(error Unsupported ARCH — piste VM = qemu_virt_aarch64)
endif

ifeq ($(LLVM),True)
  CC := clang -target $(TARGET_TRIPLE)
  LD := ld.lld
else ifneq ($(shell command -v $(TARGET_TRIPLE)-gcc 2>/dev/null),)
  CC := $(TARGET_TRIPLE)-gcc
  LD := $(TARGET_TRIPLE)-ld
else
  CC := aarch64-linux-gnu-gcc
  LD := aarch64-linux-gnu-ld
endif

MICROKIT_TOOL ?= $(MICROKIT_SDK)/bin/microkit
CFLAGS := -nostdlib -ffreestanding -g -O2 -Wall -Wno-unused-function -Werror -I$(BOARD_DIR)/include $(CFLAGS_ARCH)
LDFLAGS := -L$(BOARD_DIR)/lib --gc-sections
LIBS := -lmicrokit -Tmicrokit.ld

IMAGES := capkd.elf bus.elf gate.elf auditd.elf
IMAGE_FILE = $(BUILD_DIR)/loader.img
REPORT_FILE = $(BUILD_DIR)/report.txt

all: $(IMAGE_FILE)

$(BUILD_DIR)/%.o: %.c abi.h aos_cap.h Makefile
	$(CC) -c $(CFLAGS) $< -o $@

$(BUILD_DIR)/capkd.elf: $(BUILD_DIR)/capkd.o $(RUST_LIB)
	$(LD) $(LDFLAGS) $(BUILD_DIR)/capkd.o $(RUST_LIB) $(LIBS) -o $@

$(BUILD_DIR)/bus.elf: $(BUILD_DIR)/bus.o
	$(LD) $(LDFLAGS) $^ $(LIBS) -o $@

$(BUILD_DIR)/gate.elf: $(BUILD_DIR)/gate.o
	$(LD) $(LDFLAGS) $^ $(LIBS) -o $@

$(BUILD_DIR)/auditd.elf: $(BUILD_DIR)/auditd.o
	$(LD) $(LDFLAGS) $^ $(LIBS) -o $@

$(IMAGE_FILE) $(REPORT_FILE): $(addprefix $(BUILD_DIR)/, $(IMAGES)) aos.system
	$(MICROKIT_TOOL) aos.system --search-path $(BUILD_DIR) --board $(MICROKIT_BOARD) --config $(MICROKIT_CONFIG) -o $(IMAGE_FILE) -r $(REPORT_FILE)
