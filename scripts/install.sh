#!/bin/bash
set -euo pipefail

INSTALL_DIR="${1:-/opt/pichost}"
DATA_DIR="${2:-/var/lib/pichost}"
CONFIG_DIR="${3:-/etc/pichost}"
VERSION="${PICHOST_VERSION:-unknown}"

echo "PicHost v${VERSION} installing..."

# 1. Create directory structure
mkdir -p "$INSTALL_DIR" "$DATA_DIR" "$CONFIG_DIR"

# 2. Copy binaries
cp pichost-api pichost-worker "$INSTALL_DIR/"
chmod +x "$INSTALL_DIR"/pichost-api "$INSTALL_DIR"/pichost-worker

# 3. Copy static assets
cp -r web-ui/dist "$INSTALL_DIR/"
cp -r migrations "$INSTALL_DIR/"
if [ -d nginx ]; then cp -r nginx "$INSTALL_DIR/"; fi

# 4. Initialize .env if not present
if [ ! -f "$CONFIG_DIR/.env" ]; then
    cp .env.example "$CONFIG_DIR/.env"
    echo ">> Please edit $CONFIG_DIR/.env to configure PicHost"
    echo ">> Required: PICHOST_AUTH_JWT_SECRET (min 32 chars)"
    echo ">> Required: PICHOST_DATABASE_URL, PICHOST_REDIS_URL"
fi

# 5. Prerequisite check
echo ">> Ensure PostgreSQL 18+ and Redis 8+ are installed and running"

# 6. Install systemd services (if available)
if command -v systemctl &>/dev/null; then
    # Patch copies in a temp dir so tracked repo files stay untouched
    TMP_SVC="$(mktemp -d)"
    cp scripts/pichost-api.service scripts/pichost-worker.service "$TMP_SVC/"
    sed -i "s|/opt/pichost|$INSTALL_DIR|g" "$TMP_SVC"/*.service
    sed -i "s|/etc/pichost|$CONFIG_DIR|g" "$TMP_SVC"/*.service
    cp "$TMP_SVC"/pichost-api.service /etc/systemd/system/
    cp "$TMP_SVC"/pichost-worker.service /etc/systemd/system/
    rm -rf "$TMP_SVC"
    systemctl daemon-reload
    echo ">> systemd services installed"
    echo ">> Start:    systemctl start pichost-api pichost-worker"
    echo ">> Enable:   systemctl enable pichost-api pichost-worker"
else
    echo ">> (Non-systemd; manage manually)"
    echo ">> API:    $INSTALL_DIR/pichost-api"
    echo ">> Worker: $INSTALL_DIR/pichost-worker"
fi

echo "PicHost installation complete."
