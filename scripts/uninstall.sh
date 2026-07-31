#!/bin/bash
set -euo pipefail

INSTALL_DIR="${1:-/opt/pichost}"
DATA_DIR="${2:-/var/lib/pichost}"
CONFIG_DIR="${3:-/etc/pichost}"

echo "PicHost uninstalling..."

# 1. Stop and disable systemd services
if command -v systemctl &>/dev/null; then
    systemctl stop pichost-api pichost-worker 2>/dev/null || true
    systemctl disable pichost-api pichost-worker 2>/dev/null || true
    rm -f /etc/systemd/system/pichost-api.service
    rm -f /etc/systemd/system/pichost-worker.service
    systemctl daemon-reload
fi

# 2. Remove binaries and static files
rm -rf "$INSTALL_DIR"

# 3. Preserve data and config
echo ">> Binaries removed"
echo ">> Data dir preserved:   $DATA_DIR"
echo ">> Config dir preserved: $CONFIG_DIR"
echo ">> To fully remove: rm -rf $DATA_DIR $CONFIG_DIR"
echo "PicHost uninstall complete."
