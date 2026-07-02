# Directory Monitor - Linux Install Makefile
# Usage: sudo make install / sudo make uninstall / sudo make upgrade

PREFIX      ?= /usr/local
BIN_DIR     := $(PREFIX)/bin
CONF_DIR    := /etc/directory-monitor
DATA_DIR    := /var/lib/directory-monitor
SYSTEMD_DIR := /etc/systemd/system

APP_NAME    := directory-monitor
SERVICE     := $(APP_NAME).service

.PHONY: install uninstall upgrade status

## Install binary, config, and systemd service
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

	@cp $(SERVICE) $(SYSTEMD_DIR)/$(SERVICE)
	@systemctl daemon-reload
	@systemctl enable $(SERVICE)
	@echo "  Service: $(SERVICE) enabled"
	@echo ""
	@echo "Installation complete."
	@echo "  Edit config:  sudo nano $(CONF_DIR)/config.toml"
	@echo "  Start service: sudo systemctl start $(SERVICE)"
	@echo "  View logs:     sudo journalctl -u $(APP_NAME) -f"

## Stop service and remove all installed files
uninstall: check-root
	@echo "Uninstalling Directory Monitor..."
	@systemctl stop $(SERVICE) 2>/dev/null || true
	@systemctl disable $(SERVICE) 2>/dev/null || true
	@rm -f $(SYSTEMD_DIR)/$(SERVICE)
	@systemctl daemon-reload
	@echo "  Service removed"

	@rm -f $(BIN_DIR)/$(APP_NAME)
	@echo "  Binary removed"

	@echo ""
	@echo "Config and data preserved:"
	@echo "  $(CONF_DIR)"
	@echo "  $(DATA_DIR)"
	@echo "Remove manually if not needed."

## Stop service, replace binary, update service unit, restart
upgrade: check-root
	@echo "Upgrading Directory Monitor..."
	@systemctl stop $(SERVICE) 2>/dev/null || true
	@cp $(APP_NAME) $(BIN_DIR)/$(APP_NAME)
	@chmod 755 $(BIN_DIR)/$(APP_NAME)
	@echo "  Binary: updated"
	@cp $(SERVICE) $(SYSTEMD_DIR)/$(SERVICE)
	@echo "  Service unit: updated"
	@systemctl daemon-reload
	@echo "  Systemd: reloaded"
	@systemctl start $(SERVICE)
	@echo "Upgraded and restarted."

## Show service status
status:
	@systemctl status $(SERVICE) --no-pager 2>/dev/null || echo "Service not installed"

check-root:
	@if [ "$$(id -u)" -ne 0 ]; then \
		echo "Error: This command requires root privileges. Use: sudo make ..."; \
		exit 1; \
	fi
