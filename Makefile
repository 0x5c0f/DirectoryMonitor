# Directory Monitor - Makefile
# Usage: make [target]

APP_NAME    := directory-monitor
VERSION     := $(shell grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
DIST_DIR    := dist

# Detect platform and set build target
ifeq ($(OS),Windows_NT)
    PLATFORM  := windows
    TARGET    := x86_64-pc-windows-msvc
    EXT       := .exe
else
    UNAME_S   := $(shell uname -s)
    ifeq ($(UNAME_S),Linux)
        PLATFORM := linux
        TARGET   := x86_64-unknown-linux-musl
        EXT      :=
    endif
    ifeq ($(UNAME_S),Darwin)
        PLATFORM := macos
        TARGET   := x86_64-apple-darwin
        EXT      :=
    endif
endif

TARGET_DIR  = target/$(TARGET)/release
ARCH        := $(shell uname -m 2>/dev/null || echo x86_64)
ifeq ($(PLATFORM),linux)
    DIST_NAME := $(APP_NAME)-$(PLATFORM)-musl-$(ARCH)
else
    DIST_NAME := $(APP_NAME)-$(PLATFORM)-$(ARCH)
endif

.PHONY: all build release test fmt-check lint check clean dist install uninstall run validate help

## Default target
all: release

## Build debug version
build:
	cargo build --target $(TARGET)

## Build release version
release:
	cargo build --release --target $(TARGET)

## Run tests
test:
	cargo test

## Check rustfmt formatting
fmt-check:
	cargo fmt --check

## Run clippy linter
lint:
	cargo clippy --workspace --all-targets -- -D warnings

## Run all local quality checks
check: fmt-check lint test

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
	@cp Makefile $(DIST_DIR)/$(DIST_NAME)/Makefile
	@if [ -d docs ]; then \
		cp -r docs $(DIST_DIR)/$(DIST_NAME)/; \
	fi
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
	@echo "  make fmt-check    Check rustfmt formatting"
	@echo "  make lint         Run clippy"
	@echo "  make check        Run all quality checks (fmt + lint + test)"
	@echo "  make clean        Clean build artifacts"
	@echo "  make dist         Create distribution package"
	@echo "  make install      Install to ~/.local/bin"
	@echo "  make uninstall    Remove from ~/.local/bin"
	@echo "  make run          Run with example config"
	@echo "  make validate     Validate example config"
	@echo ""
	@echo "Platform: $(PLATFORM)-$(ARCH)"
	@echo "Target:   $(TARGET)"
