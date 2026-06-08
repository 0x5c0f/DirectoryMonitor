# Directory Monitor - Makefile
# Usage: make [target]

APP_NAME    := directory-monitor
VERSION     := $(shell grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
TARGET_DIR  := target/release
DIST_DIR    := dist

# Detect platform
ifeq ($(OS),Windows_NT)
    PLATFORM  := windows
    EXT       := .exe
else
    UNAME_S   := $(shell uname -s)
    ifeq ($(UNAME_S),Linux)
        PLATFORM := linux
        EXT      :=
    endif
    ifeq ($(UNAME_S),Darwin)
        PLATFORM := macos
        EXT      :=
    endif
endif

ARCH        := $(shell uname -m 2>/dev/null || echo x86_64)
DIST_NAME   := $(APP_NAME)-$(PLATFORM)-$(ARCH)

.PHONY: all build release test clean dist install uninstall run validate help

## Default target
all: release

## Build debug version
build:
	cargo build

## Build release version
release:
	cargo build --release

## Run tests
test:
	cargo test

## Run clippy linter
lint:
	cargo clippy -- -D warnings

## Clean build artifacts
clean:
	cargo clean
	rm -rf $(DIST_DIR)

## Create distribution package for current platform
dist: release
	@mkdir -p $(DIST_DIR)/$(DIST_NAME)
	@cp $(TARGET_DIR)/$(APP_NAME)$(EXT) $(DIST_DIR)/$(DIST_NAME)/
	@cp config.example.toml $(DIST_DIR)/$(DIST_NAME)/config.toml
	@cp packaging/README.md $(DIST_DIR)/$(DIST_NAME)/README.md
	@if [ -d packaging/$(PLATFORM) ]; then \
		cp -r packaging/$(PLATFORM)/* $(DIST_DIR)/$(DIST_NAME)/; \
	fi
	@if [ "$(PLATFORM)" = "windows" ]; then \
		cd $(DIST_DIR) && 7z a $(DIST_NAME).zip $(DIST_NAME)/ && rm -rf $(DIST_NAME); \
	else \
		tar -czf $(DIST_DIR)/$(DIST_NAME).tar.gz -C $(DIST_DIR) $(DIST_NAME) && rm -rf $(DIST_DIR)/$(DIST_NAME); \
	fi
	@echo "Created: $(DIST_DIR)/$(DIST_NAME).$(if $(filter windows,$(PLATFORM)),zip,tar.gz)"

## Install to ~/.local/bin (Linux/macOS)
install: release
	@mkdir -p ~/.local/bin
	@cp $(TARGET_DIR)/$(APP_NAME)$(EXT) ~/.local/bin/
	@echo "Installed to ~/.local/bin/$(APP_NAME)$(EXT)"

## Uninstall from ~/.local/bin
uninstall:
	@rm -f ~/.local/bin/$(APP_NAME)$(EXT)
	@echo "Uninstalled"

## Run with example config
run: release
	./$(TARGET_DIR)/$(APP_NAME)$(EXT) -c config.example.toml run

## Validate example config
validate: release
	./$(TARGET_DIR)/$(APP_NAME)$(EXT) -c config.example.toml validate

## Show help
help:
	@echo "Directory Monitor v$(VERSION)"
	@echo ""
	@echo "Targets:"
	@echo "  make              Build release (default)"
	@echo "  make build        Build debug"
	@echo "  make release      Build release"
	@echo "  make test         Run tests"
	@echo "  make lint         Run clippy"
	@echo "  make clean        Clean build artifacts"
	@echo "  make dist         Create distribution package"
	@echo "  make install      Install to ~/.local/bin"
	@echo "  make uninstall    Remove from ~/.local/bin"
	@echo "  make run          Run with example config"
	@echo "  make validate     Validate example config"
	@echo "  make help         Show this help"
	@echo ""
	@echo "Platform: $(PLATFORM)-$(ARCH)"
