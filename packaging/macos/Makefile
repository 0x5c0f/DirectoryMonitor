# Directory Monitor - macOS Install Makefile
# Usage: sudo make install / sudo make uninstall / sudo make upgrade

PREFIX      ?= /usr/local
BIN_DIR     := $(PREFIX)/bin
CONF_DIR    := /etc/directory-monitor
DATA_DIR    := /var/lib/directory-monitor
LOG_DIR     := /var/log
PLIST_DIR   := /Library/LaunchDaemons

APP_NAME    := directory-monitor
PLIST_NAME  := cc.0x5c0f.directory-monitor.plist
PLIST_LABEL := cc.0x5c0f.directory-monitor

.PHONY: install uninstall upgrade status check-root

## Install binary, config, and launchd service
install: check-root
	@echo "Installing Directory Monitor..."
	@mkdir -p $(BIN_DIR)
	@cp $(APP_NAME) $(BIN_DIR)/$(APP_NAME)
	@chmod 755 $(BIN_DIR)/$(APP_NAME)
	@echo "  Binary: $(BIN_DIR)/$(APP_NAME)"

	@mkdir -p $(CONF_DIR)
	@if [ ! -f $(CONF_DIR)/config.toml ]; then \
		cp config.toml $(CONF_DIR)/config.toml; \
		echo "  Config: $(CONF_DIR)/config.toml (new)"; \
	else \
		echo "  Config: $(CONF_DIR)/config.toml (exists, skipped)"; \
	fi

	@mkdir -p $(DATA_DIR)
	@echo "  Data:   $(DATA_DIR)"

	@cp $(PLIST_NAME) $(PLIST_DIR)/$(PLIST_NAME)
	@launchctl load $(PLIST_DIR)/$(PLIST_NAME)
	@echo "  Service: $(PLIST_NAME) loaded"
	@echo ""
	@echo "Installation complete."
	@echo "  Edit config:  sudo nano $(CONF_DIR)/config.toml"
	@echo "  View logs:    tail -f $(LOG_DIR)/$(APP_NAME).log"

## Stop service and remove all installed files
uninstall: check-root
	@echo "Uninstalling Directory Monitor..."
	@launchctl unload $(PLIST_DIR)/$(PLIST_NAME) 2>/dev/null || true
	@rm -f $(PLIST_DIR)/$(PLIST_NAME)
	@echo "  Service removed"

	@rm -f $(BIN_DIR)/$(APP_NAME)
	@echo "  Binary removed"

	@echo ""
	@echo "Config, data, and logs preserved:"
	@echo "  $(CONF_DIR)"
	@echo "  $(DATA_DIR)"
	@echo "  $(LOG_DIR)/$(APP_NAME).log"
	@echo "Remove manually if not needed."

## Stop service, replace binary, restart
upgrade: check-root
	@echo "Upgrading Directory Monitor..."
	@launchctl unload $(PLIST_DIR)/$(PLIST_NAME) 2>/dev/null || true
	@cp $(APP_NAME) $(BIN_DIR)/$(APP_NAME)
	@chmod 755 $(BIN_DIR)/$(APP_NAME)
	@launchctl load $(PLIST_DIR)/$(PLIST_NAME)
	@echo "Upgraded and restarted."

## Show service status
status:
	@launchctl list | grep $(PLIST_LABEL) || echo "Service not loaded"

check-root:
	@if [ "$$(id -u)" -ne 0 ]; then \
		echo "Error: This command requires root privileges. Use: sudo make ..."; \
		exit 1; \
	fi
