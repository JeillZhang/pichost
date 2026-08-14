#!/bin/bash
set -euo pipefail

KEEP_DATA=0
INSTALL_DIR="/opt/pichost"
CONFIG_DIR="/etc/pichost"

usage() {
    echo "Usage: $0 [--keep-data] [INSTALL_DIR] [CONFIG_DIR]"
    echo "  --keep-data   preserve $INSTALL_DIR/data (images + SQLite DB)"
}

# --- 参数解析 ---
POS=0
while [ $# -gt 0 ]; do
    case "$1" in
        --keep-data) KEEP_DATA=1 ;;
        -h|--help) usage; exit 0 ;;
        -*) echo "unknown option: $1"; usage; exit 1 ;;
        *)
            POS=$((POS + 1))
            case "$POS" in
                1) INSTALL_DIR="$1" ;;
                2) CONFIG_DIR="$1" ;;
                *) echo "error: too many positional arguments: $1"; usage; exit 1 ;;
            esac ;;
    esac
    shift
done

echo "PicHost uninstalling..."

# 1. Stop and disable systemd services
if [ "$(id -u)" = "0" ] && command -v systemctl >/dev/null 2>&1 && [ -d /run/systemd/system ]; then
    systemctl stop pichost-api pichost-worker 2>/dev/null || true
    systemctl disable pichost-api pichost-worker 2>/dev/null || true
    rm -f /etc/systemd/system/pichost-api.service
    rm -f /etc/systemd/system/pichost-worker.service
    systemctl daemon-reload
fi

# 2. data/ 处置:tty 且非 --keep-data 时确认
if [ "$KEEP_DATA" -eq 0 ] && [ -d "$INSTALL_DIR/data" ] && [ -t 0 ]; then
    read -r -p ">> $INSTALL_DIR/data 含全部图片与数据库,确认删除? [y/N] " ans || true
    [ "$ans" = "y" ] || [ "$ans" = "Y" ] || { echo ">> 取消;使用 --keep-data 保留数据"; exit 1; }
fi

# 3. Remove binaries and static files
if [ "$KEEP_DATA" -eq 1 ] && [ -d "$INSTALL_DIR/data" ]; then
    find "$INSTALL_DIR" -mindepth 1 ! -path "$INSTALL_DIR/data" ! -path "$INSTALL_DIR/data/*" -exec rm -rf {} +
    echo ">> Data preserved: $INSTALL_DIR/data (--keep-data)"
else
    rm -rf "$INSTALL_DIR"
fi

# 4. Config preserved
echo ">> Binaries removed"
echo ">> Config dir preserved: $CONFIG_DIR"
echo ">> To fully remove: rm -rf $CONFIG_DIR"
echo "PicHost uninstall complete."
